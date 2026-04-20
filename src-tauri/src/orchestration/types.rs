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
