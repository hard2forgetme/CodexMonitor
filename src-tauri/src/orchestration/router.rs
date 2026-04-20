use std::time::Instant;

use serde_json::json;
use tauri::{AppHandle, Emitter};

use super::classification::classify_task;
use super::cli_provider::call_provider;
use super::council::{dual_model_council, synapse_council};
use super::types::*;

/// The main orchestration router.
/// Routes queries through the appropriate tier pipeline based on classification.
pub(crate) struct OrchestrationRouter {
    settings: OrchestrationSettings,
}

impl OrchestrationRouter {
    pub(crate) fn new(settings: OrchestrationSettings) -> Self {
        Self { settings }
    }

    /// Process a query through the orchestration pipeline.
    pub(crate) async fn process(
        &self,
        app: &AppHandle,
        query: &str,
        cwd: Option<&str>,
        tier_override: Option<Tier>,
    ) -> Result<OrchestrationResult, String> {
        let total_start = Instant::now();

        // Step 1: Classify the task.
        emit_event(app, OrchestrationEvent::now("classification_started"));
        let mut classification = classify_task(query);

        if let Some(override_tier) = tier_override {
            classification.tier = override_tier;
            classification.reasoning = format!(
                "Tier manually set to {:?} (original: {})",
                override_tier, classification.reasoning
            );
        }

        if !self.settings.auto_tier {
            classification.tier = self.settings.default_tier;
        }

        emit_event(
            app,
            OrchestrationEvent::now("classification_completed")
                .with_data("tier", json!(classification.tier))
                .with_data("score", json!(classification.confidence))
                .with_data("synapse", json!(classification.synapse_mode)),
        );

        // Step 2: Route based on tier.
        let result = match classification.tier {
            Tier::Fast => self.execute_fast(app, query, &classification, cwd).await,
            Tier::Medium => self.execute_medium(app, query, &classification, cwd).await,
            Tier::Heavy => self.execute_heavy(app, query, &classification, cwd).await,
        }?;

        emit_event(
            app,
            OrchestrationEvent::now("orchestration_completed")
                .with_data(
                    "totalDurationMs",
                    json!(total_start.elapsed().as_millis() as u64),
                )
                .with_data("tier", json!(classification.tier)),
        );

        Ok(result)
    }

    /// Tier 1 (FAST): Single model execution.
    async fn execute_fast(
        &self,
        app: &AppHandle,
        query: &str,
        classification: &TaskClassification,
        cwd: Option<&str>,
    ) -> Result<OrchestrationResult, String> {
        let start = Instant::now();
        let tier_config = &self.settings.tier_config.fast;
        let provider = tier_config.executor;
        let config = self.get_provider_config(provider);

        emit_event(
            app,
            OrchestrationEvent::now("executor_started")
                .with_data("provider", json!(provider))
                .with_data("tier", json!("fast")),
        );

        // FAST tier uses a shorter timeout — simple queries shouldn't wait as long.
        let fast_timeout = config.timeout_ms.min(30_000);
        let response = call_provider(
            provider,
            &config,
            query,
            tier_config.executor_model.as_deref(),
            cwd,
            fast_timeout,
        )
        .await;

        let response = if !response.success {
            self.try_fallback(app, query, provider, cwd)
                .await
                .unwrap_or(response)
        } else {
            response
        };

        emit_event(
            app,
            OrchestrationEvent::now("executor_completed")
                .with_data("success", json!(response.success))
                .with_data("durationMs", json!(response.duration_ms)),
        );

        let confidence = compute_confidence(&response, None, classification.tier);

        Ok(OrchestrationResult {
            classification: classification.clone(),
            primary_response: response,
            review: None,
            council: None,
            total_duration_ms: start.elapsed().as_millis() as u64,
            confidence,
        })
    }

    /// Tier 2 (MEDIUM): Executor + reviewer dual-model council.
    async fn execute_medium(
        &self,
        app: &AppHandle,
        query: &str,
        classification: &TaskClassification,
        cwd: Option<&str>,
    ) -> Result<OrchestrationResult, String> {
        let start = Instant::now();
        let tier_config = &self.settings.tier_config.medium;

        emit_event(
            app,
            OrchestrationEvent::now("executor_started")
                .with_data("provider", json!(tier_config.executor))
                .with_data("tier", json!("medium")),
        );

        let executor_config = self.get_provider_config(tier_config.executor);
        let primary_response = call_provider(
            tier_config.executor,
            &executor_config,
            query,
            tier_config.executor_model.as_deref(),
            cwd,
            executor_config.timeout_ms,
        )
        .await;

        emit_event(
            app,
            OrchestrationEvent::now("executor_completed")
                .with_data("success", json!(primary_response.success))
                .with_data("durationMs", json!(primary_response.duration_ms)),
        );

        let review = if primary_response.success {
            if let Some(reviewer_provider) = tier_config.reviewer {
                emit_event(
                    app,
                    OrchestrationEvent::now("review_started")
                        .with_data("reviewer", json!(reviewer_provider)),
                );

                let reviewer_config = self.get_provider_config(reviewer_provider);
                let review_result = dual_model_council(
                    query,
                    &primary_response,
                    reviewer_provider,
                    &reviewer_config,
                    tier_config.reviewer_model.as_deref(),
                    cwd,
                )
                .await;

                emit_event(
                    app,
                    OrchestrationEvent::now("review_completed")
                        .with_data("status", json!(review_result.status))
                        .with_data("durationMs", json!(review_result.duration_ms)),
                );

                Some(review_result)
            } else {
                None
            }
        } else {
            None
        };

        let review_status = review.as_ref().map(|r| r.status);
        let confidence = compute_confidence(&primary_response, review_status, classification.tier);

        Ok(OrchestrationResult {
            classification: classification.clone(),
            primary_response,
            review,
            council: None,
            total_duration_ms: start.elapsed().as_millis() as u64,
            confidence,
        })
    }

    /// Tier 3 (HEAVY): Full SYNAPSE multi-agent council.
    async fn execute_heavy(
        &self,
        app: &AppHandle,
        query: &str,
        classification: &TaskClassification,
        cwd: Option<&str>,
    ) -> Result<OrchestrationResult, String> {
        let start = Instant::now();

        if classification.synapse_mode {
            emit_event(
                app,
                OrchestrationEvent::now("council_started")
                    .with_data("mode", json!("synapse")),
            );

            let council_result = synapse_council(query, &self.settings, cwd).await;

            emit_event(
                app,
                OrchestrationEvent::now("council_completed")
                    .with_data("members", json!(council_result.members.len()))
                    .with_data("rounds", json!(council_result.rounds.len()))
                    .with_data("durationMs", json!(council_result.total_duration_ms)),
            );

            let primary_response = ProviderResponse {
                success: !council_result.synthesis.is_empty(),
                output: council_result.synthesis.clone(),
                thinking: None,
                tool_calls: vec![],
                model_used: "synapse-council".to_string(),
                provider: self.settings.tier_config.heavy.executor,
                duration_ms: council_result.total_duration_ms,
                error: None,
            };

            Ok(OrchestrationResult {
                classification: classification.clone(),
                primary_response,
                review: None,
                council: Some(council_result),
                total_duration_ms: start.elapsed().as_millis() as u64,
                confidence: 0.90,
            })
        } else {
            let medium_result = self
                .execute_medium(app, query, classification, cwd)
                .await?;
            Ok(OrchestrationResult {
                total_duration_ms: start.elapsed().as_millis() as u64,
                ..medium_result
            })
        }
    }

    /// Attempt fallback providers when the primary fails.
    async fn try_fallback(
        &self,
        app: &AppHandle,
        query: &str,
        failed_provider: Provider,
        cwd: Option<&str>,
    ) -> Option<ProviderResponse> {
        let fallback_chain = match failed_provider {
            Provider::Gemini => vec![Provider::Claude, Provider::Codex],
            Provider::Claude => vec![Provider::Codex, Provider::Gemini],
            Provider::Codex => vec![Provider::Claude, Provider::Gemini],
        };

        for fallback in fallback_chain {
            let config = self.get_provider_config(fallback);
            if !config.enabled {
                continue;
            }

            emit_event(
                app,
                OrchestrationEvent::now("fallback_started")
                    .with_data("provider", json!(fallback)),
            );

            let response = call_provider(
                fallback,
                &config,
                query,
                config.default_model.as_deref(),
                cwd,
                config.timeout_ms,
            )
            .await;

            if response.success {
                emit_event(
                    app,
                    OrchestrationEvent::now("fallback_succeeded")
                        .with_data("provider", json!(fallback)),
                );
                return Some(response);
            }
        }

        None
    }

    fn get_provider_config(&self, provider: Provider) -> ProviderConfig {
        match provider {
            Provider::Claude => self.settings.providers.claude.clone(),
            Provider::Gemini => self.settings.providers.gemini.clone(),
            Provider::Codex => self.settings.providers.codex.clone(),
        }
    }
}

/// Compute response confidence using multi-factor scoring.
///
/// Factors: success state, response substance (non-empty, non-trivial),
/// review verdict, tier ceiling, and error presence.
fn compute_confidence(
    response: &ProviderResponse,
    review_status: Option<ReviewStatus>,
    tier: Tier,
) -> f64 {
    if !response.success {
        return 0.0;
    }

    // Base confidence for a successful response.
    let mut confidence: f64 = 0.70;

    // Substance check: penalize very short responses (likely incomplete).
    let char_count = response.output.len();
    if char_count < 50 {
        confidence -= 0.10;
    } else if char_count > 500 {
        confidence += 0.05;
    }

    // Error field present even on success → partial failure.
    if response.error.is_some() {
        confidence -= 0.15;
    }

    // Review verdict is the strongest signal.
    match review_status {
        Some(ReviewStatus::Approved) => confidence += 0.15,
        Some(ReviewStatus::NeedsRevision) => confidence -= 0.15,
        None => {}
    }

    // Tier ceiling: single-model answers can't claim full certainty.
    match tier {
        Tier::Fast => confidence = confidence.min(0.80),
        Tier::Medium => confidence = confidence.min(0.92),
        Tier::Heavy => {}
    }

    confidence.clamp(0.0, 1.0)
}

fn emit_event(app: &AppHandle, event: OrchestrationEvent) {
    let _ = app.emit("orchestration_event", &event);
}
