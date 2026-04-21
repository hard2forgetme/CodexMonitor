//! Re-exports shared types from crate::types and defines orchestration-only types.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export the shared types that live in crate::types (visible to daemon bins too).
pub(crate) use crate::types::{
    OrchestrationProvider as Provider,
    OrchestrationProviderConfig as ProviderConfig,
    OrchestrationProviderSettings as ProviderSettings,
    OrchestrationSettings,
    OrchestrationTier as Tier,
    OrchestrationTierConfig as TierConfig,
    OrchestrationTierSettings as TierSettings,
};

/// Task classification result from the heuristic engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TaskClassification {
    pub(crate) complexity: u8,
    pub(crate) risk: u8,
    pub(crate) length: u8,
    pub(crate) expertise: u8,
    pub(crate) tier: Tier,
    pub(crate) confidence: f64,
    pub(crate) reasoning: String,
    pub(crate) synapse_mode: bool,
}

/// Normalized response from any CLI provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderResponse {
    pub(crate) success: bool,
    pub(crate) output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) thinking: Option<String>,
    pub(crate) tool_calls: Vec<ToolCallEntry>,
    pub(crate) model_used: String,
    pub(crate) provider: Provider,
    pub(crate) duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

/// A tool call extracted from provider output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolCallEntry {
    #[serde(rename = "type")]
    pub(crate) call_type: String,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) output: Option<String>,
}

/// Role a council member plays in SYNAPSE mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CouncilRole {
    Architect,
    Engineer,
    Security,
    Analyst,
    Critic,
    Strategist,
    Rogue,
    Watcher,
    Reaper,
}

/// A member of a SYNAPSE council.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CouncilMember {
    pub(crate) agent_id: String,
    pub(crate) model_id: String,
    pub(crate) provider: Provider,
    pub(crate) role: CouncilRole,
    pub(crate) system_prompt: String,
}

/// A single round in a council debate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DebateRound {
    pub(crate) round_number: u32,
    pub(crate) round_type: String,
    pub(crate) member_id: String,
    pub(crate) content: String,
    pub(crate) timestamp_ms: u64,
}

/// The full orchestration result returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrchestrationResult {
    pub(crate) classification: TaskClassification,
    pub(crate) primary_response: ProviderResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) review: Option<ReviewResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) council: Option<CouncilResult>,
    pub(crate) total_duration_ms: u64,
    pub(crate) confidence: f64,
}

/// Result of a dual-model review (Tier 2).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewResult {
    pub(crate) reviewer: Provider,
    pub(crate) reviewer_model: String,
    pub(crate) status: ReviewStatus,
    pub(crate) suggestions: String,
    pub(crate) duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReviewStatus {
    Approved,
    NeedsRevision,
}

/// Full SYNAPSE council result (Tier 3).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CouncilResult {
    pub(crate) members: Vec<CouncilMember>,
    pub(crate) rounds: Vec<DebateRound>,
    pub(crate) synthesis: String,
    pub(crate) total_duration_ms: u64,
}

/// Streaming event emitted during orchestration for real-time UI updates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrchestrationEvent {
    #[serde(rename = "type")]
    pub(crate) event_type: String,
    pub(crate) timestamp_ms: u64,
    #[serde(default)]
    pub(crate) data: HashMap<String, serde_json::Value>,
}

impl OrchestrationEvent {
    pub(crate) fn now(event_type: &str) -> Self {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            event_type: event_type.to_string(),
            timestamp_ms: ts,
            data: HashMap::new(),
        }
    }

    pub(crate) fn with_data(mut self, key: &str, value: serde_json::Value) -> Self {
        self.data.insert(key.to_string(), value);
        self
    }
}

/// A single prior conversation turn, forwarded from the frontend to give
/// orchestration pipelines the context of the thread the user is working in.
/// Kept intentionally minimal: no images, no tool call dumps, no diffs —
/// only the textual gist, so token budget stays predictable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContextMsg {
    /// "user" | "assistant" | "system" — free-form, used only for labelling.
    pub(crate) role: String,
    /// Plain-text content, already truncated / sanitized by the frontend.
    pub(crate) text: String,
}

/// Render a compact transcript from prior context messages. Returns an empty
/// string when no context is provided, so callers can safely concatenate.
pub(crate) fn format_context_transcript(context: Option<&[ContextMsg]>) -> String {
    let Some(items) = context else {
        return String::new();
    };
    if items.is_empty() {
        return String::new();
    }
    let mut out = String::from("Recent conversation (most recent last):\n");
    for msg in items {
        let role = if msg.role.is_empty() {
            "user"
        } else {
            msg.role.as_str()
        };
        // Cap any single message at 2000 chars to keep things tidy.
        let text = if msg.text.len() > 2000 {
            let mut trimmed = msg.text[..2000].to_string();
            trimmed.push_str("… [truncated]");
            trimmed
        } else {
            msg.text.clone()
        };
        out.push_str(&format!("- {role}: {text}\n"));
    }
    out.push('\n');
    out
}

/// Combine prior context with the current user request in a single string
/// suitable for passing as the full prompt to a provider. When there is no
/// context, returns the query unchanged.
pub(crate) fn query_with_context(query: &str, context: Option<&[ContextMsg]>) -> String {
    let transcript = format_context_transcript(context);
    if transcript.is_empty() {
        query.to_string()
    } else {
        format!("{transcript}Current request: {query}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, text: &str) -> ContextMsg {
        ContextMsg {
            role: role.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn transcript_is_empty_when_no_context() {
        assert_eq!(format_context_transcript(None), "");
        assert_eq!(format_context_transcript(Some(&[])), "");
    }

    #[test]
    fn transcript_lists_messages_in_order() {
        let ctx = vec![
            msg("user", "hello"),
            msg("assistant", "hi!"),
            msg("user", "follow up"),
        ];
        let t = format_context_transcript(Some(&ctx));
        assert!(t.starts_with("Recent conversation"));
        let hello_pos = t.find("hello").unwrap();
        let hi_pos = t.find("hi!").unwrap();
        let follow_pos = t.find("follow up").unwrap();
        assert!(hello_pos < hi_pos && hi_pos < follow_pos);
    }

    #[test]
    fn transcript_labels_blank_role_as_user() {
        let ctx = vec![msg("", "anonymous")];
        let t = format_context_transcript(Some(&ctx));
        assert!(t.contains("- user: anonymous"));
    }

    #[test]
    fn transcript_truncates_long_messages() {
        let long = "x".repeat(3000);
        let ctx = vec![msg("assistant", &long)];
        let t = format_context_transcript(Some(&ctx));
        assert!(t.contains("[truncated]"));
        assert!(t.len() < 3000 + 100);
    }

    #[test]
    fn query_with_context_passthrough_when_empty() {
        assert_eq!(query_with_context("do it", None), "do it");
        assert_eq!(query_with_context("do it", Some(&[])), "do it");
    }

    #[test]
    fn query_with_context_prepends_transcript() {
        let ctx = vec![msg("user", "previous")];
        let result = query_with_context("do it", Some(&ctx));
        assert!(result.starts_with("Recent conversation"));
        assert!(result.ends_with("Current request: do it"));
        assert!(result.contains("previous"));
    }
}
