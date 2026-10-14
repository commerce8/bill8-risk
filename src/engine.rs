// SPDX-FileCopyrightText: Copyright Logically Magic Inc. <hello@logicallymagic.com>
// SPDX-FileCopyrightText: Copyright Hamid Jahanzad <Hamid@Jahanzad.com>
// SPDX-FileCopyrightText: Copyright Contributors to the Bill8:Risk
// SPDX-License-Identifier: Apache-2.0

//! The rules evaluator — PURE and fail-closed.
//!
//! No I/O, no row locks: this module runs on the payments hot path.
//! Any evaluation error (malformed params, unknown predicate, wrong
//! types) evaluates to **block** — never allow. See risk DESIGN.md §8.

use bill8::risk::contracts::Rule;
use rust_decimal::Decimal;

/// The evaluated outcome of a rule set against a charge context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// No rule matched — proceed.
    Allow,
    /// A rule matched with action=review.
    Review,
    /// A rule matched with action=block.
    Block,
}

/// Evaluate the ordered rule set against a charge context. First match
/// by priority wins. A MALFORMED rule (unknown predicate, bad params)
/// evaluates to **block** regardless of its declared action — fail-closed
/// means the declared `allow` of a rule that cannot be evaluated is
/// never honored.
pub fn evaluate(rules: &[Rule], context: &serde_json::Value) -> Decision {
    for rule in rules {
        if !rule.enabled {
            continue;
        }
        // A malformed rule matches with a forced block (the caller cannot
        // distinguish "matched cleanly" from "failed to evaluate" — by
        // design, both block).
        let (matched, malformed) = matches_or_malformed(rule, context);
        if malformed {
            return Decision::Block;
        }
        if !matched {
            continue;
        }
        return match rule.action.as_str() {
            "allow" => Decision::Allow,
            "review" => Decision::Review,
            // Unknown action spellings block (fail-closed).
            _ => Decision::Block,
        };
    }
    Decision::Allow
}

/// Whether a single rule matches the context, and whether the rule is
/// malformed (unknown predicate or unparseable params/context). A
/// malformed rule reports `matched=false, malformed=true` — the caller
/// blocks on it without consulting its action.
fn matches_or_malformed(rule: &Rule, context: &serde_json::Value) -> (bool, bool) {
    match rule.predicate.as_str() {
        "amount_above" => {
            let Some(threshold) = rule.params["threshold"]
                .as_str()
                .and_then(|s| Decimal::from_str_exact(s).ok())
            else {
                return (false, true); // malformed threshold → block
            };
            let Some(amount) = context["amount"]
                .as_str()
                .and_then(|s| Decimal::from_str_exact(s).ok())
            else {
                return (false, true); // malformed context → block
            };
            (amount > threshold, false)
        }
        "country_in" => {
            let Some(list) = rule.params["countries"].as_array() else {
                return (false, true);
            };
            let Some(country) = context["country"].as_str() else {
                return (false, true);
            };
            (list.iter().any(|c| c.as_str() == Some(country)), false)
        }
        "velocity_above" => {
            let Some(limit) = rule.params["limit"].as_i64() else {
                return (false, true);
            };
            let Some(count) = context["velocity"].as_i64() else {
                return (false, true);
            };
            (count > limit, false)
        }
        // Unknown predicate → malformed → block (fail-closed).
        _ => (false, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(predicate: &str, params: serde_json::Value, action: &str) -> Rule {
        Rule {
            id: uuid::Uuid::new_v4(),
            env_id: uuid::Uuid::new_v4(),
            predicate: predicate.into(),
            params,
            action: action.into(),
            priority: 100,
            enabled: true,
            created_at: chrono::Utc::now(),
        }
    }

    fn ctx(amount: &str) -> serde_json::Value {
        serde_json::json!({ "amount": amount, "country": "US", "velocity": 1 })
    }

    #[test]
    fn no_rules_allows() {
        assert_eq!(evaluate(&[], &ctx("1000.00")), Decision::Allow);
    }

    #[test]
    fn amount_threshold_blocks() {
        let rules = vec![rule(
            "amount_above",
            serde_json::json!({ "threshold": "5000.00" }),
            "block",
        )];
        assert_eq!(evaluate(&rules, &ctx("12000.00")), Decision::Block);
        assert_eq!(evaluate(&rules, &ctx("100.00")), Decision::Allow);
    }

    #[test]
    fn malformed_rule_fails_closed() {
        // Missing threshold → malformed → must block, never allow.
        let rules = vec![rule("amount_above", serde_json::json!({}), "block")];
        assert_eq!(evaluate(&rules, &ctx("1.00")), Decision::Block);
    }

    #[test]
    fn unknown_predicate_fails_closed() {
        let rules = vec![rule("nonsense", serde_json::json!({}), "allow")];
        // Even an "allow" action with an unknown predicate blocks — the
        // predicate itself is malformed.
        assert_eq!(evaluate(&rules, &ctx("1.00")), Decision::Block);
    }

    #[test]
    fn unknown_action_blocks() {
        let rules = vec![rule(
            "amount_above",
            serde_json::json!({ "threshold": "1.00" }),
            "nonsense-action",
        )];
        assert_eq!(evaluate(&rules, &ctx("2.00")), Decision::Block);
    }

    #[test]
    fn country_list_reviews() {
        let rules = vec![rule(
            "country_in",
            serde_json::json!({ "countries": ["XX", "YY"] }),
            "review",
        )];
        let ctx_x = serde_json::json!({ "amount": "10.00", "country": "XX" });
        assert_eq!(evaluate(&rules, &ctx_x), Decision::Review);
        assert_eq!(evaluate(&rules, &ctx("10.00")), Decision::Allow);
    }

    #[test]
    fn disabled_rule_skipped() {
        let mut r = rule(
            "amount_above",
            serde_json::json!({ "threshold": "1.00" }),
            "block",
        );
        r.enabled = false;
        assert_eq!(evaluate(&[r], &ctx("10000.00")), Decision::Allow);
    }

    #[test]
    fn velocity_window() {
        let rules = vec![rule(
            "velocity_above",
            serde_json::json!({ "limit": 10 }),
            "block",
        )];
        let hot = serde_json::json!({ "amount": "10.00", "velocity": 11 });
        assert_eq!(evaluate(&rules, &hot), Decision::Block);
    }
}
