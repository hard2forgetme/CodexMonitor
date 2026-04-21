use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::state::AppState;

use super::classification::classify_task;
use super::router::OrchestrationRouter;
use super::types::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrchestrationRequest {
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) cwd: Option<String>,
    #[serde(default)]
    pub(crate) tier_override: Option<Tier>,
    /// Recent conversation turns (most recent last), forwarded so that
    /// orchestration pipelines see the thread the user is working in.
    #[serde(default)]
    pub(crate) context: Option<Vec<ContextMsg>>,
}

/// Get the current orchestration settings.
#[tauri::command]
pub(crate) async fn get_orchestration_settings(
    state: State<'_, AppState>,
) -> Result<OrchestrationSettings, String> {
    let settings = state.app_settings.lock().await;
    Ok(settings.orchestration.clone())
}

/// Update orchestration settings.
#[tauri::command]
pub(crate) async fn update_orchestration_settings(
    orchestration: OrchestrationSettings,
    state: State<'_, AppState>,
) -> Result<OrchestrationSettings, String> {
    let mut settings = state.app_settings.lock().await;
    settings.orchestration = orchestration.clone();

    // Persist settings.
    let json = serde_json::to_string_pretty(&*settings)
        .map_err(|e| format!("Failed to serialize settings: {e}"))?;
    tokio::fs::write(&state.settings_path, json)
        .await
        .map_err(|e| format!("Failed to write settings: {e}"))?;

    Ok(orchestration)
}

/// Classify a query without executing it.
/// Useful for showing the user what tier would be selected.
#[tauri::command]
pub(crate) async fn classify_orchestration_task(
    query: String,
) -> Result<TaskClassification, String> {
    Ok(classify_task(&query))
}

/// Execute the full orchestration pipeline for a query.
#[tauri::command]
pub(crate) async fn run_orchestration(
    request: OrchestrationRequest,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<OrchestrationResult, String> {
    let settings = {
        let app_settings = state.app_settings.lock().await;
        app_settings.orchestration.clone()
    };

    if !settings.enabled {
        return Err("Orchestration is not enabled. Enable it in Settings > Orchestration.".to_string());
    }

    let router = OrchestrationRouter::new(settings);
    router
        .process(
            &app,
            &request.query,
            request.cwd.as_deref(),
            request.tier_override,
            request.context.as_deref(),
        )
        .await
}

/// Execute the GARMR agent pipeline for MEDIUM/HEAVY tasks:
/// 1. Gemini creates a plan (workhorse/ideation)
/// 2. Claude executes in full agent mode with that plan (heavy coding)
/// 3. Returns Claude's execution result with the council context
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GarmrAgentRequest {
    pub(crate) query: String,
    pub(crate) cwd: Option<String>,
    pub(crate) tier: Tier,
    /// Recent conversation turns (most recent last), included in the planner,
    /// executor, and reviewer prompts so follow-ups understand the thread.
    #[serde(default)]
    pub(crate) context: Option<Vec<ContextMsg>>,
}

#[tauri::command]
pub(crate) async fn run_garmr_agent(
    request: GarmrAgentRequest,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<OrchestrationResult, String> {
    use super::cli_provider::{call_claude_agent, call_provider};

    let settings = {
        let app_settings = state.app_settings.lock().await;
        app_settings.orchestration.clone()
    };

    if !settings.enabled {
        return Err("Orchestration is not enabled.".to_string());
    }

    let start = std::time::Instant::now();
    // Skip re-classification — the frontend already classified and passed the tier.
    let classification = TaskClassification {
        complexity: 0,
        risk: 0,
        length: 0,
        expertise: 0,
        tier: request.tier,
        confidence: 0.90,
        reasoning: format!("Tier set to {:?} by frontend classification", request.tier),
        synapse_mode: request.tier == Tier::Heavy,
    };
    let cwd = request.cwd.as_deref();

    // Step 1: Gemini creates the plan (ideation/workhorse)
    let gemini_config = settings.providers.gemini.clone();
    let planner_model = match request.tier {
        Tier::Heavy => settings.tier_config.medium.reviewer_model.as_deref(),
        _ => settings.tier_config.medium.reviewer_model.as_deref(),
    };

    let transcript = format_context_transcript(request.context.as_deref());
    let planning_prompt = format!(
        "You are the GARMR planning council. Analyze this task and create a detailed, \
         actionable execution plan. Be specific about what files to change, what logic to add, \
         and what order to work in. The plan will be given to an AI coding agent that will \
         execute it against the actual codebase.\n\n\
         {transcript}\
         Task: {task}\n\n\
         Create a thorough plan with clear steps.",
        task = request.query
    );

    emit_event(
        &app,
        OrchestrationEvent::now("planning_started")
            .with_data("provider", serde_json::json!("gemini"))
            .with_data("tier", serde_json::json!(format!("{:?}", request.tier))),
    );

    let plan_response = call_provider(
        Provider::Gemini,
        &gemini_config,
        &planning_prompt,
        planner_model,
        cwd,
        gemini_config.timeout_ms,
    )
    .await;

    emit_event(
        &app,
        OrchestrationEvent::now("planning_completed")
            .with_data("success", serde_json::json!(plan_response.success)),
    );

    let plan_text = if plan_response.success {
        plan_response.output.clone()
    } else {
        // If Gemini fails, still proceed — Claude can work without a plan
        String::new()
    };

    // Step 2: Claude executes in full agent mode with the plan as system context
    let claude_config = settings.providers.claude.clone();
    let executor_model = match request.tier {
        Tier::Heavy => settings.tier_config.heavy.executor_model.as_deref(),
        _ => settings.tier_config.medium.executor_model.as_deref(),
    };

    let system_prompt = if plan_text.is_empty() && transcript.is_empty() {
        None
    } else {
        let plan_section = if plan_text.is_empty() {
            String::new()
        } else {
            format!(
                "=== COUNCIL PLAN ===\n{plan_text}\n=== END PLAN ===\n\n\
                 Work through the plan step by step. Edit files, run tests, verify your changes.\n\n"
            )
        };
        Some(format!(
            "{transcript}{plan_section}\
             You have a GARMR council plan and/or conversation context above. \
             Execute thoroughly against the codebase."
        ))
    };

    emit_event(
        &app,
        OrchestrationEvent::now("executor_started")
            .with_data("provider", serde_json::json!("claude"))
            .with_data("mode", serde_json::json!("agent")),
    );

    let agent_response = call_claude_agent(
        &claude_config,
        &request.query,
        system_prompt.as_deref(),
        executor_model,
        cwd,
        claude_config.timeout_ms,
    )
    .await;

    emit_event(
        &app,
        OrchestrationEvent::now("executor_completed")
            .with_data("success", serde_json::json!(agent_response.success))
            .with_data("durationMs", serde_json::json!(agent_response.duration_ms)),
    );

    // Step 3: Optionally have Gemini review (for MEDIUM tier)
    let review = if request.tier == Tier::Medium && agent_response.success {
        let review_prompt = format!(
            "Review this AI agent's work on the task. Was it thorough? Any issues?\n\n\
             {transcript}\
             Original task: {task}\n\nAgent output: {output}",
            task = request.query,
            output = agent_response.output
        );
        let review_response = call_provider(
            Provider::Gemini,
            &gemini_config,
            &review_prompt,
            planner_model,
            cwd,
            gemini_config.timeout_ms,
        )
        .await;

        if review_response.success {
            Some(ReviewResult {
                reviewer: Provider::Gemini,
                reviewer_model: review_response.model_used.clone(),
                status: ReviewStatus::Approved,
                suggestions: review_response.output.clone(),
                duration_ms: review_response.duration_ms,
            })
        } else {
            None
        }
    } else {
        None
    };

    let confidence = if agent_response.success { 0.85 } else { 0.40 };

    Ok(OrchestrationResult {
        classification,
        primary_response: agent_response,
        review,
        council: None,
        total_duration_ms: start.elapsed().as_millis() as u64,
        confidence,
    })
}

fn emit_event(app: &AppHandle, event: OrchestrationEvent) {
    let _ = app.emit("orchestration_event", &event);
}

/// Check which providers are available (binary exists on PATH).
#[tauri::command]
pub(crate) async fn check_provider_availability(
    state: State<'_, AppState>,
) -> Result<ProviderAvailability, String> {
    let settings = state.app_settings.lock().await;
    let orch = &settings.orchestration;

    let claude_bin = orch
        .providers
        .claude
        .binary
        .as_deref()
        .unwrap_or("claude");
    let gemini_bin = orch
        .providers
        .gemini
        .binary
        .as_deref()
        .unwrap_or("gemini");
    let codex_bin = orch
        .providers
        .codex
        .binary
        .as_deref()
        .unwrap_or("codex");

    let (claude_ok, gemini_ok, codex_ok) = tokio::join!(
        check_binary(claude_bin),
        check_binary(gemini_bin),
        check_binary(codex_bin),
    );

    Ok(ProviderAvailability {
        claude: claude_ok,
        gemini: gemini_ok,
        codex: codex_ok,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderAvailability {
    pub(crate) claude: bool,
    pub(crate) gemini: bool,
    pub(crate) codex: bool,
}

async fn check_binary(name: &str) -> bool {
    let which_cmd = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    tokio::process::Command::new(which_cmd)
        .arg(name)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}
