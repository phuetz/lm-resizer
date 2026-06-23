//! Signal-retention benchmark (no LLM, deterministic, CI-runnable).
//!
//! Honest framing of what query-aware compression buys: lm-resizer compacts a
//! uniform array **losslessly** by default (it keeps every row, just in a
//! denser form), so with no budget the query is neutral — every relevant row
//! already survives. The query earns its keep under pressure, when the
//! compressor must *drop* rows: it then biases retention toward the rows the
//! user asked about (anchors: ids, tickets, quoted terms, emails).
//!
//! This benchmark runs the real compressor on a labeled fixture (a noisy array
//! + a query + the row the user asked about) and asserts the deterministic,
//! always-true property: the query-relevant row survives compression, and the
//! output is genuinely compressed. It prints a query-vs-blind table so the
//! effect (and the lossless-is-neutral honesty) is visible. No mocks, no model.
//!
//! Run: `cargo test -p lm-resizer-core --test retention -- --nocapture` for the table.

use lm_resizer_core::transforms::smart_crusher::{SmartCrusher, SmartCrusherConfig};

struct Case {
    name: &'static str,
    input: String,
    query: &'static str,
    /// The distinctive value the user's question is about; must survive.
    must_keep: String,
}

/// Uniform-schema, low-cardinality array (compressible, mirrors the crate's own
/// crusher tests): `id`, `name` (mostly `hay-{i}`, one carries `marker`),
/// `score` (i % 7).
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
    let uuid = "550e8400-e29b-41d4-a716-446655440000";
    let ticket = "ISSUE-90210";
    vec![
        Case {
            name: "uuid-anchor",
            input: array_with_marker(120, 61, uuid),
            query: "what does the entry 550e8400-e29b-41d4-a716-446655440000 say?",
            must_keep: uuid.to_string(),
        },
        Case {
            name: "ticket-anchor",
            input: array_with_marker(120, 73, ticket),
            query: "summarize the entry for ISSUE-90210",
            must_keep: ticket.to_string(),
        },
    ]
}

#[test]
fn query_relevant_rows_survive_compression() {
    // Default compressor — the real shipped behavior.
    let crusher = SmartCrusher::new(SmartCrusherConfig::default());

    eprintln!(
        "{:<16} {:>9} {:>9} {:>10} {:>10}",
        "case", "query?", "blind?", "in(B)", "out(B)"
    );
    for c in &cases() {
        let with_query = crusher.crush(&c.input, c.query, 0.0);
        let blind = crusher.crush(&c.input, "", 0.0);

        let in_q = with_query.compressed.contains(&c.must_keep);
        let in_b = blind.compressed.contains(&c.must_keep);

        eprintln!(
            "{:<16} {:>9} {:>9} {:>10} {:>10}",
            c.name,
            if in_q { "kept" } else { "DROPPED" },
            if in_b { "kept" } else { "DROPPED" },
            c.input.len(),
            with_query.compressed.len(),
        );

        // Deterministic, always-true claims:
        // 1) the compressor actually ran and compacted the array,
        assert!(
            with_query.was_modified && with_query.compressed.len() < c.input.len(),
            "{}: expected the array to be compressed",
            c.name
        );
        // 2) the query-relevant row survives compression (query plumbed through).
        assert!(
            in_q,
            "{}: query-relevant row must survive query-aware compression",
            c.name
        );
    }
}
