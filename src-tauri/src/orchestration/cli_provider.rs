use std::time::Instant;

use serde_json::Value;
use tokio::process::Command;

use super::types::{Provider, ProviderConfig, ProviderResponse, ToolCallEntry};

/// Call the appropriate CLI provider based on the provider enum.
pub(crate) async fn call_provider(
    provider: Provider,
    config: &ProviderConfig,
    prompt: &str,
    model: Option<&str>,
    cwd: Option<&str>,
    timeout_ms: u64,
) -> ProviderResponse {
    match provider {
        Provider::Claude => call_claude(config, prompt, model, cwd, timeout_ms).await,
        Provider::Gemini => call_gemini(config, prompt, model, cwd, timeout_ms).await,
        Provider::Codex => call_codex(config, prompt, model, cwd, timeout_ms).await,
    }
}

// ---------------------------------------------------------------------------
// Claude adapter — uses `claude --print --output-format json`
// ---------------------------------------------------------------------------

async fn call_claude(
    config: &ProviderConfig,
    prompt: &str,
    model: Option<&str>,
    cwd: Option<&str>,
    timeout_ms: u64,
) -> ProviderResponse {
    let binary = config.binary.as_deref().unwrap_or("claude");
    let model_id = model
        .or(config.default_model.as_deref())
        .unwrap_or("claude-sonnet-4-6");

    let mut cmd = Command::new(binary);
    cmd.args(["--print", "--output-format", "json", "--model", model_id]);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let start = Instant::now();

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            return error_response(Provider::Claude, model_id, &start, &format!("Failed to spawn claude: {err}"));
        }
    };

    match run_with_stdin(child, prompt, timeout_ms).await {
        Ok(output) => parse_claude_output(&output, model_id, start),
        Err(err) => error_response(Provider::Claude, model_id, &start, &err),
    }
}

fn parse_claude_output(raw: &str, model_id: &str, start: Instant) -> ProviderResponse {
    let trimmed = raw.trim();

    // Claude --print --output-format json may prepend thinking/reasoning text
    // before the JSON blob. We scan from the end to find the last valid JSON
    // object containing a "result" key, which is the actual response.
    for json_start in find_json_candidates(trimmed) {
        if let Ok(data) = serde_json::from_str::<Value>(&trimmed[json_start..]) {
            if let Some(result_text) = data.get("result").and_then(|v| v.as_str()) {
                return ProviderResponse {
                    success: true,
                    output: result_text.to_string(),
                    thinking: None,
                    tool_calls: vec![],
                    model_used: data
                        .get("model")
                        .and_then(|v| v.as_str())
                        .unwrap_or(model_id)
                        .to_string(),
                    provider: Provider::Claude,
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: None,
                };
            }
        }
    }

    ProviderResponse {
        success: !trimmed.is_empty(),
        output: trimmed.to_string(),
        thinking: None,
        tool_calls: vec![],
        model_used: model_id.to_string(),
        provider: Provider::Claude,
        duration_ms: start.elapsed().as_millis() as u64,
        error: if trimmed.is_empty() {
            Some("empty response".to_string())
        } else {
            None
        },
    }
}

/// Find all `{` positions in the string, yielding them from last to first.
/// This lets us preferentially parse the final JSON object (the actual
/// response) rather than any `{` characters in thinking/reasoning text.
fn find_json_candidates(s: &str) -> Vec<usize> {
    let mut positions: Vec<usize> = s.match_indices('{').map(|(i, _)| i).collect();
    positions.reverse();
    positions
}

// ---------------------------------------------------------------------------
// Claude AGENT adapter — runs Claude Code in full agent mode with tool use.
// Unlike call_claude (text-only --print), this spawns a real agent session
// that can read/edit files, run bash commands, and test code.
// ---------------------------------------------------------------------------

pub(crate) async fn call_claude_agent(
    config: &ProviderConfig,
    prompt: &str,
    system_prompt: Option<&str>,
    model: Option<&str>,
    cwd: Option<&str>,
    timeout_ms: u64,
) -> ProviderResponse {
    let binary = config.binary.as_deref().unwrap_or("claude");
    let model_id = model
        .or(config.default_model.as_deref())
        .unwrap_or("claude-sonnet-4-6");

    let mut cmd = Command::new(binary);
    cmd.args([
        "--print",
        "--output-format", "json",
        "--model", model_id,
        "--permission-mode", "bypassPermissions",
        "--no-session-persistence",
    ]);

    // Inject the council plan as a system-level prompt so Claude
    // treats it as authoritative guidance, not user conversation.
    if let Some(sys) = system_prompt {
        cmd.args(["--system-prompt", sys]);
    }

    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let start = Instant::now();

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            return error_response(
                Provider::Claude,
                model_id,
                &start,
                &format!("Failed to spawn claude agent: {err}"),
            );
        }
    };

    match run_with_stdin(child, prompt, timeout_ms).await {
        Ok(output) => parse_claude_output(&output, model_id, start),
        Err(err) => error_response(Provider::Claude, model_id, &start, &err),
    }
}

// ---------------------------------------------------------------------------
// Gemini adapter — uses `gemini` CLI
// ---------------------------------------------------------------------------

async fn call_gemini(
    config: &ProviderConfig,
    prompt: &str,
    model: Option<&str>,
    cwd: Option<&str>,
    timeout_ms: u64,
) -> ProviderResponse {
    let binary = config.binary.as_deref().unwrap_or("gemini");
    let model_id = model
        .or(config.default_model.as_deref())
        .unwrap_or("gemini-3-flash-preview");

    let mut cmd = Command::new(binary);
    // Pass the prompt directly via -p instead of stdin.
    // Gemini CLI treats -p as non-interactive (headless) mode with the prompt
    // value as the query. This is more reliable than piping through stdin.
    cmd.args([
        "--yolo",
        "--model", model_id,
        "--output-format", "json",
        "--allowed-mcp-server-names", "__none__",
        "-p", prompt,
    ]);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let start = Instant::now();

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            return error_response(Provider::Gemini, model_id, &start, &format!("Failed to spawn gemini: {err}"));
        }
    };

    match run_without_stdin(child, timeout_ms).await {
        Ok(output) => parse_gemini_output(&output, model_id, start),
        Err(err) => error_response(Provider::Gemini, model_id, &start, &err),
    }
}

fn parse_gemini_output(raw: &str, model_id: &str, start: Instant) -> ProviderResponse {
    let trimmed = raw.trim();

    if let Some(json_start) = trimmed.find('{') {
        if let Ok(data) = serde_json::from_str::<Value>(&trimmed[json_start..]) {
            let response_text = data
                .get("response")
                .or_else(|| data.get("result"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let thinking = data
                .get("thinking")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            return ProviderResponse {
                success: true,
                output: response_text,
                thinking,
                tool_calls: vec![],
                model_used: data
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or(model_id)
                    .to_string(),
                provider: Provider::Gemini,
                duration_ms: start.elapsed().as_millis() as u64,
                error: None,
            };
        }
    }

    ProviderResponse {
        success: !trimmed.is_empty(),
        output: trimmed.to_string(),
        thinking: None,
        tool_calls: vec![],
        model_used: model_id.to_string(),
        provider: Provider::Gemini,
        duration_ms: start.elapsed().as_millis() as u64,
        error: if trimmed.is_empty() {
            Some("empty response".to_string())
        } else {
            None
        },
    }
}

// ---------------------------------------------------------------------------
// Codex adapter — uses `codex exec --json`
// ---------------------------------------------------------------------------

async fn call_codex(
    config: &ProviderConfig,
    prompt: &str,
    model: Option<&str>,
    cwd: Option<&str>,
    timeout_ms: u64,
) -> ProviderResponse {
    let binary = config.binary.as_deref().unwrap_or("codex");
    let model_id = model
        .or(config.default_model.as_deref())
        .unwrap_or("gpt-5.4-medium");

    let mut cmd = Command::new(binary);
    cmd.args([
        "exec",
        "--skip-git-repo-check",
        "--full-auto",
        "--json",
        "-m",
        model_id,
        prompt,
    ]);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let start = Instant::now();

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(err) => {
            return error_response(Provider::Codex, model_id, &start, &format!("Failed to spawn codex: {err}"));
        }
    };

    match run_without_stdin(child, timeout_ms).await {
        Ok(output) => parse_codex_output(&output, model_id, start),
        Err(err) => error_response(Provider::Codex, model_id, &start, &err),
    }
}

fn parse_codex_output(raw: &str, model_id: &str, start: Instant) -> ProviderResponse {
    let mut final_output = String::new();
    let mut thinking_blocks = Vec::new();
    let mut tool_calls = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };

        let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let item_type = event
            .get("item")
            .and_then(|i| i.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if event_type == "item.completed" && item_type == "agent_message" {
            if let Some(text) = event.get("item").and_then(|i| i.get("text")).and_then(|v| v.as_str()) {
                final_output = text.to_string();
            }
        }

        if item_type == "reasoning" {
            if let Some(text) = event.get("item").and_then(|i| i.get("text")).and_then(|v| v.as_str()) {
                thinking_blocks.push(text.to_string());
            }
        }

        if matches!(item_type, "action" | "tool_call" | "shell_command") {
            let name = event
                .get("item")
                .and_then(|i| i.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let status = event
                .get("item")
                .and_then(|i| i.get("status"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let output = event
                .get("item")
                .and_then(|i| i.get("output"))
                .and_then(|v| v.as_str())
                .map(|s| s.chars().take(500).collect::<String>());
            tool_calls.push(ToolCallEntry {
                call_type: item_type.to_string(),
                name,
                status,
                output,
            });
        }
    }

    let thinking = if thinking_blocks.is_empty() {
        None
    } else {
        Some(thinking_blocks.join("\n\n"))
    };

    ProviderResponse {
        success: !final_output.is_empty(),
        output: final_output,
        thinking,
        tool_calls,
        model_used: model_id.to_string(),
        provider: Provider::Codex,
        duration_ms: start.elapsed().as_millis() as u64,
        error: None,
    }
}

// ---------------------------------------------------------------------------
// Shared subprocess helpers
// ---------------------------------------------------------------------------

fn error_response(provider: Provider, model_id: &str, start: &Instant, err: &str) -> ProviderResponse {
    ProviderResponse {
        success: false,
        output: String::new(),
        thinking: None,
        tool_calls: vec![],
        model_used: model_id.to_string(),
        provider,
        duration_ms: start.elapsed().as_millis() as u64,
        error: Some(err.to_string()),
    }
}

/// Send SIGTERM, wait briefly, then SIGKILL if the process is still alive.
fn kill_process_tree(pid: u32) {
    #[cfg(unix)]
    {
        use std::process::Command as StdCommand;
        // SIGTERM the process group (negative PID targets the group).
        let _ = StdCommand::new("kill")
            .args(["-TERM", "--", &format!("-{pid}")])
            .status();
        // Brief grace period, then SIGKILL.
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = StdCommand::new("kill")
            .args(["-9", "--", &format!("-{pid}")])
            .status();
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .status();
    }
}

async fn run_with_stdin(
    mut child: tokio::process::Child,
    input: &str,
    timeout_ms: u64,
) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;

    let id = child.id();

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes()).await;
        let _ = stdin.shutdown().await;
    }

    let timeout = std::time::Duration::from_millis(timeout_ms);
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if output.status.success() || !stdout.trim().is_empty() {
                Ok(stdout)
            } else {
                Err(format!(
                    "process exited with code {}: {}",
                    output.status.code().unwrap_or(-1),
                    stderr
                ))
            }
        }
        Ok(Err(err)) => Err(format!("process error: {err}")),
        Err(_) => {
            if let Some(pid) = id {
                kill_process_tree(pid);
            }
            Err(format!("timed out after {timeout_ms}ms"))
        }
    }
}

async fn run_without_stdin(
    child: tokio::process::Child,
    timeout_ms: u64,
) -> Result<String, String> {
    let timeout = std::time::Duration::from_millis(timeout_ms);
    let id = child.id();
    match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            if output.status.success() || !stdout.trim().is_empty() {
                Ok(stdout)
            } else {
                Err(format!(
                    "process exited with code {}: {}",
                    output.status.code().unwrap_or(-1),
                    stderr
                ))
            }
        }
        Ok(Err(err)) => Err(format!("process error: {err}")),
        Err(_) => {
            if let Some(pid) = id {
                kill_process_tree(pid);
            }
            Err(format!("timed out after {timeout_ms}ms"))
        }
    }
}
