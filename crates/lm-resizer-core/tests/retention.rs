//! Signal-retention benchmark (no LLM, deterministic, CI-runnable).
//!
//! Proves the query-aware advance under budget pressure. With no budget,
//! lm-resizer compresses losslessly (keeps every row) so the query is neutral.
//! Under a token budget the compressor MUST drop rows to fit — and that is
//! where the user's question pays off: it biases retention toward the rows the
//! user asked about (anchors: UUIDs, ids, quoted terms, emails).
//!
//! This benchmark runs the real SmartCrusher in budget mode on labeled fixtures
//! (a 200-row array + a query that references one anchored row + a tight budget)
//! and asserts the headline contrast: **query-aware keeps the relevant row that
//! blind compression drops.** No mocks, no model.
//!
//! Run: `cargo test -p lm-resizer-core --test retention -- --nocapture` for the table.

use lm_resizer_core::transforms::smart_crusher::{SmartCrusher, SmartCrusherConfig};

struct Case {
    name: &'static str,
    input: String,
    query: String,
    /// Distinctive anchor value the user's question is about; must survive.
    must_keep: String,
}

/// 200-row array; `marker_idx` (a middle row, beyond first/last keep-rules)
/// carries `marker` as its `name`. Other rows are `hay-{i}`.
fn array_with_marker(rows: usize, marker_idx: usize, marker: &str) -> String {
    let mut out = String::from("[");
    for i in 0..rows {
        if i > 0 {
            out.push(',');
        }
        let name = if i == marker_idx {
            marker.to_string()
        } else {
            format!("hay-{i}")
        };
        out.push_str(&format!(
            r#"{{"id":{i},"name":"{name}","score":{}}}"#,
            i % 7
        ));
    }
    out.push(']');
    out
}

fn cases() -> Vec<Case> {
    let uuid_a = "550e8400-e29b-41d4-a716-446655440000";
    let uuid_b = "f47ac10b-58cc-4372-a567-0e02b2c3d479";
    vec![
        Case {
            name: "uuid-A@151",
            input: array_with_marker(200, 151, uuid_a),
            query: format!("tell me about entry {uuid_a}"),
            must_keep: uuid_a.to_string(),
        },
        Case {
            name: "uuid-B@137",
            input: array_with_marker(200, 137, uuid_b),
            query: format!("what happened with {uuid_b}?"),
            must_keep: uuid_b.to_string(),
        },
    ]
}

/// A crusher with a tight token budget — forces lossy row-dropping to fit.
fn budget_crusher(budget_tokens: usize) -> SmartCrusher {
    let mut cfg = SmartCrusherConfig::default();
    cfg.budget_tokens = Some(budget_tokens);
    SmartCrusher::new(cfg)
}

#[test]
fn query_aware_keeps_relevant_row_that_blind_drops_under_budget() {
    let crusher = budget_crusher(60);
    let mut kept_query = 0usize;
    let mut kept_blind = 0usize;

    eprintln!(
        "{:<14} {:>9} {:>9} {:>10} {:>10}",
        "case", "query?", "blind?", "in(B)", "out(B)"
    );
    for c in &cases() {
        let with_query = crusher.crush(&c.input, &c.query, 0.0);
        let blind = crusher.crush(&c.input, "", 0.0);

        let in_q = with_query.compressed.contains(&c.must_keep);
        let in_b = blind.compressed.contains(&c.must_keep);
        if in_q {
            kept_query += 1;
        }
        if in_b {
            kept_blind += 1;
        }

        eprintln!(
            "{:<14} {:>9} {:>9} {:>10} {:>10}",
            c.name,
            if in_q { "kept" } else { "DROPPED" },
            if in_b { "kept" } else { "DROPPED" },
            c.input.len(),
            with_query.compressed.len(),
        );

        // Budget forces heavy lossy compression on the 200-row array.
        assert!(
            with_query.was_modified && with_query.compressed.len() < c.input.len() / 4,
            "{}: expected heavy budget-driven compression",
            c.name
        );
        // The headline: the user's question keeps its row alive...
        assert!(
            in_q,
            "{}: query-aware must keep the query-relevant row",
            c.name
        );
        // ...while blind compression drops it.
        assert!(
            !in_b,
            "{}: blind compression was expected to drop the (middle) anchor row",
            c.name
        );
    }

    eprintln!("relevant rows kept — query-aware: {kept_query}/2, blind: {kept_blind}/2");
    assert!(
        kept_query > kept_blind,
        "query-aware must retain strictly more relevant rows than blind under budget \
         (query {kept_query} vs blind {kept_blind})"
    );
}
