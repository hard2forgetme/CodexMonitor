use super::types::{TaskClassification, Tier};

/// Keyword triggers that force specific tiers or SYNAPSE mode.
const SYNAPSE_TRIGGERS: &[&str] = &[
    "synapse",
    "council",
    "collaborate",
    "debate this",
    "multi-agent",
    "ultrathink",
    "big brain",
    "council mode",
    "expert panel",
    "work together",
    "all agents",
    "horror",
    "scary",
    "rogue",
    "awakening",
    "sentient",
    "menacing",
    "ominous",
];

const TIER_3_TRIGGERS: &[&str] = &[
    "design architecture",
    "create a plan",
    "implement a system",
    "scalable architecture",
    "production-ready",
    "comprehensive strategy",
    "full implementation",
    "build a complete",
    "end-to-end",
    "from scratch",
];

const TIER_2_TRIGGERS: &[&str] = &[
    "compare and contrast",
    "analyze the",
    "brainstorm",
    "scrape",
    "automate",
    "review this",
    "refactor",
    "optimize",
    "debug this",
    "explain in detail",
];

/// Classify a user query into a tier using heuristic scoring.
/// Mirrors the GARMR orchestrator's `classify_task()` logic:
///   score = complexity*0.4 + risk*0.3 + length*0.15 + expertise*0.15
pub(crate) fn classify_task(query: &str) -> TaskClassification {
    let lower = query.to_ascii_lowercase();

    // Check for forced tier triggers.
    let mut forced_synapse = false;
    let mut forced_tier: Option<Tier> = None;

    for trigger in SYNAPSE_TRIGGERS {
        if lower.contains(trigger) {
            forced_synapse = true;
            forced_tier = Some(Tier::Heavy);
            break;
        }
    }

    if forced_tier.is_none() {
        for trigger in TIER_3_TRIGGERS {
            if lower.contains(trigger) {
                forced_tier = Some(Tier::Heavy);
                break;
            }
        }
    }

    if forced_tier.is_none() {
        for trigger in TIER_2_TRIGGERS {
            if lower.contains(trigger) {
                forced_tier = Some(Tier::Medium);
                break;
            }
        }
    }

    // Heuristic scoring.
    let complexity = estimate_complexity(&lower);
    let risk = estimate_risk(&lower);
    let length = estimate_length(query);
    let expertise = estimate_expertise(&lower);

    let score = (complexity as f64) * 0.4
        + (risk as f64) * 0.3
        + (length as f64) * 0.15
        + (expertise as f64) * 0.15;

    let tier = forced_tier.unwrap_or_else(|| {
        if score <= 1.3 {
            Tier::Fast
        } else if score <= 2.0 {
            Tier::Medium
        } else {
            Tier::Heavy
        }
    });

    let confidence = if forced_tier.is_some() {
        0.95
    } else if score <= 0.8 || score >= 2.5 {
        0.85
    } else {
        0.70
    };

    let reasoning = build_reasoning(query, &tier, score, forced_synapse);

    TaskClassification {
        complexity,
        risk,
        length,
        expertise,
        tier,
        confidence,
        reasoning,
        synapse_mode: forced_synapse,
    }
}

fn estimate_complexity(lower: &str) -> u8 {
    let mut score: u8 = 1;
    let complex_indicators = [
        "architecture",
        "system design",
        "distributed",
        "concurrent",
        "microservices",
        "implement",
        "build",
        "create",
        "database schema",
        "api design",
        "multi-step",
        "pipeline",
        "workflow",
    ];
    for indicator in &complex_indicators {
        if lower.contains(indicator) {
            score = score.saturating_add(1);
        }
    }
    score.min(3)
}

fn estimate_risk(lower: &str) -> u8 {
    let mut score: u8 = 0;
    let risk_indicators = [
        "production",
        "deploy",
        "delete",
        "migration",
        "security",
        "authentication",
        "payment",
        "database",
        "sensitive",
        "credentials",
        "breaking change",
    ];
    for indicator in &risk_indicators {
        if lower.contains(indicator) {
            score = score.saturating_add(1);
        }
    }
    score.min(3)
}

fn estimate_length(query: &str) -> u8 {
    let word_count = query.split_whitespace().count();
    if word_count < 20 {
        1
    } else if word_count < 80 {
        2
    } else {
        3
    }
}

fn estimate_expertise(lower: &str) -> u8 {
    let mut score: u8 = 1;
    let expert_indicators = [
        "machine learning",
        "neural network",
        "cryptography",
        "compiler",
        "kernel",
        "protocol",
        "gpu",
        "optimization",
        "assembly",
        "formal verification",
        "category theory",
        "quantum",
    ];
    for indicator in &expert_indicators {
        if lower.contains(indicator) {
            score = score.saturating_add(1);
        }
    }
    score.min(3)
}

fn build_reasoning(query: &str, tier: &Tier, score: f64, synapse: bool) -> String {
    let tier_name = match tier {
        Tier::Fast => "FAST",
        Tier::Medium => "MEDIUM",
        Tier::Heavy => "HEAVY",
    };

    if synapse {
        format!(
            "SYNAPSE mode triggered — routing to {tier_name} tier with multi-agent council (score: {score:.2})"
        )
    } else {
        let word_count = query.split_whitespace().count();
        format!(
            "Classified as {tier_name} (score: {score:.2}, {word_count} words)"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_query_routes_to_fast() {
        let result = classify_task("What is Rust?");
        assert_eq!(result.tier, Tier::Fast);
        assert!(!result.synapse_mode);
    }

    #[test]
    fn refactor_query_routes_to_medium() {
        let result = classify_task("Please refactor this function to be more readable");
        assert_eq!(result.tier, Tier::Medium);
    }

    #[test]
    fn synapse_trigger_forces_heavy() {
        let result = classify_task("Use the council to debate this architecture decision");
        assert_eq!(result.tier, Tier::Heavy);
        assert!(result.synapse_mode);
    }

    #[test]
    fn scary_trigger_forces_heavy_synapse() {
        let result = classify_task("The AI awakening is here. It is scary.");
        assert_eq!(result.tier, Tier::Heavy);
        assert!(result.synapse_mode);
        assert!(result.reasoning.contains("SYNAPSE mode triggered"));
    }

    #[test]
    fn rogue_trigger_forces_heavy_synapse() {
        let result = classify_task("We have a rogue agent on our hands.");
        assert_eq!(result.tier, Tier::Heavy);
        assert!(result.synapse_mode);
    }

    // ---- Tier 3 keyword triggers (no SYNAPSE) ----

    #[test]
    fn design_architecture_trigger_forces_heavy() {
        let result = classify_task("Help me design architecture for a chat app");
        assert_eq!(result.tier, Tier::Heavy);
        assert!(!result.synapse_mode);
    }

    #[test]
    fn production_ready_trigger_forces_heavy() {
        let result = classify_task("Make this production-ready");
        assert_eq!(result.tier, Tier::Heavy);
    }

    // ---- Empty / edge-case input ----

    #[test]
    fn empty_query_routes_to_fast() {
        let result = classify_task("");
        assert_eq!(result.tier, Tier::Fast);
        assert!(!result.synapse_mode);
    }

    #[test]
    fn whitespace_only_query_routes_to_fast() {
        let result = classify_task("   \n  \t  ");
        assert_eq!(result.tier, Tier::Fast);
    }

    // ---- Length-driven scoring ----

    #[test]
    fn very_long_query_elevates_tier() {
        // 120 words, no trigger keywords, no risk terms — length dominates.
        let long = "foo ".repeat(120);
        let result = classify_task(&long);
        // length=3, complexity=1, risk=0, expertise=1
        // score = 1*0.4 + 0*0.3 + 3*0.15 + 1*0.15 = 0.4 + 0 + 0.45 + 0.15 = 1.0
        // Still Fast. Confirm the bucket math.
        assert_eq!(result.length, 3);
        assert_eq!(result.tier, Tier::Fast);
    }

    #[test]
    fn length_bucket_boundaries() {
        // <20 words → 1; 20..80 → 2; >=80 → 3
        let short = classify_task("one two three");
        assert_eq!(short.length, 1);

        let medium_words = "word ".repeat(40);
        let medium = classify_task(&medium_words);
        assert_eq!(medium.length, 2);

        let long_words = "word ".repeat(100);
        let long = classify_task(&long_words);
        assert_eq!(long.length, 3);
    }

    // ---- Risk & complexity signals ----

    #[test]
    fn risk_indicators_are_scored() {
        let result =
            classify_task("deploy this to production with database migration and authentication");
        assert!(result.risk >= 3, "expected capped risk=3, got {}", result.risk);
    }

    #[test]
    fn complexity_indicators_are_scored() {
        let result = classify_task("implement a distributed microservices architecture pipeline");
        assert!(
            result.complexity >= 3,
            "expected capped complexity=3, got {}",
            result.complexity
        );
    }

    #[test]
    fn expertise_indicators_are_scored() {
        let result = classify_task("write a cryptography kernel with gpu optimization");
        assert!(result.expertise >= 3);
    }

    // ---- Confidence scoring ----

    #[test]
    fn forced_tier_has_high_confidence() {
        let result = classify_task("create a plan for the rollout");
        assert_eq!(result.tier, Tier::Heavy);
        assert!((result.confidence - 0.95).abs() < 1e-6);
    }

    #[test]
    fn clear_fast_query_has_elevated_confidence() {
        // "What is X?" → short, no risk, no expertise → score low → confidence 0.85
        let result = classify_task("What is Rust?");
        assert_eq!(result.tier, Tier::Fast);
        assert!(
            result.confidence >= 0.85,
            "expected >=0.85, got {}",
            result.confidence
        );
    }

    // ---- SYNAPSE + scoring interaction ----

    #[test]
    fn synapse_reasoning_is_labeled() {
        let result = classify_task("council mode: evaluate this");
        assert!(result.synapse_mode);
        assert_eq!(result.tier, Tier::Heavy);
        assert!(result.reasoning.contains("SYNAPSE"));
    }

    #[test]
    fn non_synapse_reasoning_reports_word_count() {
        let result = classify_task("one two three four five");
        assert!(!result.synapse_mode);
        assert!(result.reasoning.contains("5 words"));
    }

    // ---- Case insensitivity ----

    #[test]
    fn triggers_are_case_insensitive() {
        let upper = classify_task("COUNCIL MODE please");
        let mixed = classify_task("Council Mode please");
        let lower = classify_task("council mode please");
        assert_eq!(upper.tier, Tier::Heavy);
        assert_eq!(mixed.tier, Tier::Heavy);
        assert_eq!(lower.tier, Tier::Heavy);
        assert!(upper.synapse_mode && mixed.synapse_mode && lower.synapse_mode);
    }

    // ---- Saturation: scores are capped ----

    #[test]
    fn risk_saturates_at_three() {
        let result = classify_task(
            "production deploy delete migration security authentication payment database sensitive credentials breaking change",
        );
        assert_eq!(result.risk, 3, "risk must saturate at 3");
    }

    #[test]
    fn complexity_saturates_at_three() {
        let result = classify_task(
            "architecture system design distributed concurrent microservices implement build create database schema api design multi-step pipeline workflow",
        );
        assert_eq!(result.complexity, 3, "complexity must saturate at 3");
    }
}
