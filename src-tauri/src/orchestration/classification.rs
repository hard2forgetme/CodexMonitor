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
}
