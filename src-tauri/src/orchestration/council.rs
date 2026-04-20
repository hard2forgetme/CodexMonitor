use std::time::Instant;

use super::cli_provider::call_provider;
use super::types::*;

/// Execute a dual-model council (Tier 2).
/// The executor produces the primary response, then the reviewer evaluates it.
pub(crate) async fn dual_model_council(
    query: &str,
    executor_response: &ProviderResponse,
    reviewer_provider: Provider,
    reviewer_config: &ProviderConfig,
    reviewer_model: Option<&str>,
    cwd: Option<&str>,
) -> ReviewResult {
    let start = Instant::now();

    let review_prompt = format!(
        "You are a code reviewer. Review this response and provide feedback.\n\n\
         ORIGINAL REQUEST:\n{query}\n\n\
         RESPONSE TO REVIEW:\n{output}\n\n\
         Evaluate the response for:\n\
         1. Correctness — is it factually/technically accurate?\n\
         2. Completeness — does it fully address the request?\n\
         3. Quality — is it well-structured and clear?\n\n\
         Reply with a JSON object:\n\
         {{\"status\": \"APPROVED\" or \"NEEDS_REVISION\", \"suggestions\": \"your feedback here\"}}",
        output = executor_response.output
    );

    let response = call_provider(
        reviewer_provider,
        reviewer_config,
        &review_prompt,
        reviewer_model,
        cwd,
        reviewer_config.timeout_ms,
    )
    .await;

    let (status, suggestions) = parse_review_response(&response.output);

    ReviewResult {
        reviewer: reviewer_provider,
        reviewer_model: response.model_used,
        status,
        suggestions,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

fn parse_review_response(output: &str) -> (ReviewStatus, String) {
    if let Some(json_start) = output.find('{') {
        if let Some(json_end) = output.rfind('}') {
            if let Ok(data) =
                serde_json::from_str::<serde_json::Value>(&output[json_start..=json_end])
            {
                let status = match data.get("status").and_then(|v| v.as_str()) {
                    Some(s) if s.to_uppercase().contains("APPROVED") => ReviewStatus::Approved,
                    Some(s) if s.to_uppercase().contains("REVISION") => ReviewStatus::NeedsRevision,
                    Some(_) => ReviewStatus::NeedsRevision, // Unrecognized status → fail-safe
                    None => ReviewStatus::NeedsRevision,    // Missing field → fail-safe
                };
                let suggestions = data
                    .get("suggestions")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                return (status, suggestions);
            }
        }
    }

    // Fallback: scan for explicit approval language; default to needs_revision (fail-safe).
    let lower = output.to_ascii_lowercase();
    if lower.contains("approved") && !lower.contains("not approved") {
        (ReviewStatus::Approved, output.to_string())
    } else {
        (ReviewStatus::NeedsRevision, output.to_string())
    }
}

/// Execute a full SYNAPSE council (Tier 3).
/// Three agents propose solutions, cross-examine, then a chairperson synthesizes.
pub(crate) async fn synapse_council(
    query: &str,
    settings: &OrchestrationSettings,
    cwd: Option<&str>,
) -> CouncilResult {
    let start = Instant::now();

    let members = cast_council(query, settings);
    let proposals = run_proposal_round(query, &members, settings, cwd).await;
    let critiques = run_critique_round(query, &members, &proposals, settings, cwd).await;
    let synthesis = run_synthesis_round(query, &proposals, &critiques, settings, cwd).await;

    let mut all_rounds = Vec::new();
    all_rounds.extend(proposals);
    all_rounds.extend(critiques);

    CouncilResult {
        members,
        rounds: all_rounds,
        synthesis,
        total_duration_ms: start.elapsed().as_millis() as u64,
    }
}

fn cast_council(query: &str, settings: &OrchestrationSettings) -> Vec<CouncilMember> {
    let lower = query.to_ascii_lowercase();
    let mut members = Vec::new();

    // Architect: focuses on system design, scalability, and architecture trade-offs.
    if settings.providers.gemini.enabled {
        members.push(CouncilMember {
            agent_id: "gemini-architect".to_string(),
            model_id: settings
                .tier_config
                .heavy
                .executor_model
                .clone()
                .unwrap_or_else(|| "gemini-3-flash-preview".to_string()),
            provider: Provider::Gemini,
            role: CouncilRole::Architect,
            system_prompt: "You are the Architect. Focus on system design, scalability, \
                           maintainability, and architectural trade-offs. Propose clean \
                           abstractions and identify structural risks. Be precise and actionable."
                .to_string(),
        });
    }

    // Engineer: focuses on implementation quality, correctness, and pragmatism.
    if settings.providers.codex.enabled {
        members.push(CouncilMember {
            agent_id: "codex-engineer".to_string(),
            model_id: settings
                .providers
                .codex
                .default_model
                .clone()
                .unwrap_or_else(|| "gpt-5.4-medium".to_string()),
            provider: Provider::Codex,
            role: CouncilRole::Engineer,
            system_prompt: "You are the Engineer. Focus on implementation correctness, \
                           performance, error handling, and pragmatic solutions. Identify \
                           edge cases and suggest concrete code-level improvements."
                .to_string(),
        });
    }

    // Third member role is context-dependent.
    let (third_role, third_prompt) = if lower.contains("security") || lower.contains("auth")
        || lower.contains("credentials") || lower.contains("vulnerability")
    {
        (CouncilRole::Watcher, "You are the Security Analyst. Focus on attack surfaces, \
                                 authentication flaws, data exposure risks, and OWASP vulnerabilities. \
                                 Be thorough and specific about threat vectors.")
    } else {
        (CouncilRole::Critic, "You are the Critic. Challenge assumptions, find logical flaws, \
                               identify hidden risks, and stress-test proposals. Be constructive \
                               but rigorous — flag what others might miss.")
    };

    if settings.providers.claude.enabled {
        members.push(CouncilMember {
            agent_id: "claude-specialist".to_string(),
            model_id: settings
                .providers
                .claude
                .default_model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string()),
            provider: Provider::Claude,
            role: third_role,
            system_prompt: third_prompt.to_string(),
        });
    }

    if members.is_empty() {
        members.push(CouncilMember {
            agent_id: "claude-sole".to_string(),
            model_id: "claude-sonnet-4-6".to_string(),
            provider: Provider::Claude,
            role: CouncilRole::Engineer,
            system_prompt: "You are a senior software engineer. Provide thorough, \
                           well-reasoned solutions with attention to correctness and edge cases."
                .to_string(),
        });
    }

    members
}

async fn run_proposal_round(
    query: &str,
    members: &[CouncilMember],
    settings: &OrchestrationSettings,
    cwd: Option<&str>,
) -> Vec<DebateRound> {
    let now_ms = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    };

    let mut handles = Vec::new();

    for member in members {
        let prompt = format!(
            "{system}\n\nPlease provide your proposal for the following task:\n\n{query}",
            system = member.system_prompt
        );
        let provider = member.provider;
        let config = get_provider_config(provider, settings);
        let model = member.model_id.clone();
        let member_id = member.agent_id.clone();
        let cwd = cwd.map(|s| s.to_string());

        handles.push(tokio::spawn(async move {
            let response = call_provider(
                provider,
                &config,
                &prompt,
                Some(&model),
                cwd.as_deref(),
                config.timeout_ms,
            )
            .await;
            (member_id, response.output)
        }));
    }

    let mut rounds = Vec::new();
    for handle in handles {
        match handle.await {
            Ok((member_id, content)) => {
                rounds.push(DebateRound {
                    round_number: 1,
                    round_type: "proposal".to_string(),
                    member_id,
                    content,
                    timestamp_ms: now_ms(),
                });
            }
            Err(err) => {
                eprintln!("[GARMR] Council proposal task failed: {err}");
                rounds.push(DebateRound {
                    round_number: 1,
                    round_type: "proposal".to_string(),
                    member_id: "unknown".to_string(),
                    content: format!("[COUNCIL ERROR] Member failed: {err}"),
                    timestamp_ms: now_ms(),
                });
            }
        }
    }

    rounds
}

async fn run_critique_round(
    query: &str,
    members: &[CouncilMember],
    proposals: &[DebateRound],
    settings: &OrchestrationSettings,
    cwd: Option<&str>,
) -> Vec<DebateRound> {
    let now_ms = || {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    };

    let proposals_text: String = proposals
        .iter()
        .map(|p| format!("[{}]: {}", p.member_id, p.content))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let mut handles = Vec::new();

    for member in members {
        let prompt = format!(
            "{system}\n\n\
             The following proposals were made for this task:\n\n\
             TASK: {query}\n\n\
             PROPOSALS:\n{proposals_text}\n\n\
             Please critique each proposal. Identify strengths, weaknesses, \
             and potential issues. Be constructive but thorough.",
            system = member.system_prompt,
        );
        let provider = member.provider;
        let config = get_provider_config(provider, settings);
        let model = member.model_id.clone();
        let member_id = member.agent_id.clone();
        let cwd = cwd.map(|s| s.to_string());

        handles.push(tokio::spawn(async move {
            let response = call_provider(
                provider,
                &config,
                &prompt,
                Some(&model),
                cwd.as_deref(),
                config.timeout_ms,
            )
            .await;
            (member_id, response.output)
        }));
    }

    let mut rounds = Vec::new();
    for handle in handles {
        match handle.await {
            Ok((member_id, content)) => {
                rounds.push(DebateRound {
                    round_number: 2,
                    round_type: "critique".to_string(),
                    member_id,
                    content,
                    timestamp_ms: now_ms(),
                });
            }
            Err(err) => {
                eprintln!("[GARMR] Council critique task failed: {err}");
                rounds.push(DebateRound {
                    round_number: 2,
                    round_type: "critique".to_string(),
                    member_id: "unknown".to_string(),
                    content: format!("[COUNCIL ERROR] Member failed: {err}"),
                    timestamp_ms: now_ms(),
                });
            }
        }
    }

    rounds
}

async fn run_synthesis_round(
    query: &str,
    proposals: &[DebateRound],
    critiques: &[DebateRound],
    settings: &OrchestrationSettings,
    cwd: Option<&str>,
) -> String {
    let proposals_text: String = proposals
        .iter()
        .map(|p| format!("[{}]: {}", p.member_id, p.content))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let critiques_text: String = critiques
        .iter()
        .map(|c| format!("[{}]: {}", c.member_id, c.content))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    let synthesis_prompt = format!(
        "You are the synthesis lead. Merge the best elements from these proposals \
         while addressing the critiques. Produce a single, actionable final answer.\n\n\
         ORIGINAL TASK:\n{query}\n\n\
         PROPOSALS:\n{proposals_text}\n\n\
         CRITIQUES:\n{critiques_text}\n\n\
         Synthesize a comprehensive response that:\n\
         1. Incorporates the strongest ideas from each proposal\n\
         2. Addresses the valid concerns raised in critiques\n\
         3. Resolves any conflicts between proposals\n\
         4. Provides a clear, actionable result"
    );

    let executor_provider = settings.tier_config.heavy.executor;
    let config = get_provider_config(executor_provider, settings);
    let model = settings
        .tier_config
        .heavy
        .executor_model
        .as_deref()
        .unwrap_or("claude-opus-4-6");

    // Synthesis processes all proposals + critiques, so give it 2x the normal timeout.
    let synthesis_timeout = config.timeout_ms.saturating_mul(2);
    let response = call_provider(
        executor_provider,
        &config,
        &synthesis_prompt,
        Some(model),
        cwd,
        synthesis_timeout,
    )
    .await;

    response.output
}

fn get_provider_config(provider: Provider, settings: &OrchestrationSettings) -> ProviderConfig {
    match provider {
        Provider::Claude => settings.providers.claude.clone(),
        Provider::Gemini => settings.providers.gemini.clone(),
        Provider::Codex => settings.providers.codex.clone(),
    }
}
