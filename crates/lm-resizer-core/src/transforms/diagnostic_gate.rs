//! Failure lines are not a compression budget.
//!
//! Generic compressors rank lines by frequency or position. A CI log puts its
//! verdict at the end, once: measured on two real GitHub Actions logs, the log
//! compressor kept 0/2 and 2/10 `##[error]` lines — through `compress`, the MCP
//! tool, and the HTTP proxy's live zone alike. The omission was announced and
//! the original was recoverable, but the diagnostic was no longer in front of
//! the model.
//!
//! The gate does not undo the compression: failure lines that the compressed
//! text no longer shows are appended in their original order, under
//! a marker. One definition of "failure line" serves every path.

use regex::Regex;
use std::sync::LazyLock;

/// Lines that report a failure.
pub static FAILURE_SIGNAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)##\[error\]|\berror\b|\bfail(ed|ures?|s)?\b|\bFAIL\b|✘|✗|×|exception|panic|traceback|\bassertion\b|\b(expected|actual|received)\b|:line\s+[0-9]+|\bvalues differ\b|\bdiff(erence)?\b|exit code [1-9]|timed? ?out|\b\w*error\b|(?m)^\s*(E\s+|assert\b)|called .*unwrap\(\)|(?m)^\s*[\w./\\-]+\.[[:alpha:]]\w*:\d+(:\d+)?|(?:\bat\s+|❯\s+).+:\d+(:\d+)?|\breturned a non-zero code\b|\bcancell?ed\b|npm ERR!",
    )
    .expect("valid failure regex")
});

/// `Label: value` — how test frameworks print what they compared:
/// `Collection: ["alpha", "beta"]`, `Not found: "gamma"`, `Locator: …`,
/// `Call log:`. Such a line carries no failure word, and a word list cannot
/// enumerate every framework's labels; its position after a failure line is
/// what marks it as an explanation.
static LABELLED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\[[^\]]*\]\s*)?[A-Za-z][A-Za-z ]{0,28}:(\s|$)").expect("valid label regex")
});

static SGR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").expect("valid SGR regex"));

fn is_failure_signal(line: &str) -> bool {
    if line.contains('\u{1b}') {
        FAILURE_SIGNAL.is_match(&SGR.replace_all(line, ""))
    } else {
        FAILURE_SIGNAL.is_match(line)
    }
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Failure lines of `before` that `after` no longer shows, each with the
/// explanation lines that follow it in `before` and that `after` lost too.
/// With indentation and multiplicity, in the original order. SGR color codes
/// do not count as missing information.
///
/// An explanation line comes right after a failure line (or after another
/// explanation line), before any blank line, and is either labelled
/// (`Collection: …`) or indented deeper than the failure line. Measured on a
/// real `dotnet test --logger detailed`: extending the failure word list with
/// `expected|actual` (review fix of 23/09) left `Collection:` and `Not found:`
/// lost on the generic paths; following the explanation recovers them.
pub fn lost_failure_lines(before: &str, after: &str) -> Vec<String> {
    if before == after {
        return Vec::new();
    }
    let lines: Vec<&str> = before.lines().collect();
    let mut keep = vec![false; lines.len()];
    let usable = |l: &str| !l.trim().is_empty();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if !(usable(line) && is_failure_signal(line))
            || line.trim().starts_with("% Total    % Received")
            || line.trim().starts_with("diff --git ")
            || line.trim().starts_with("test result: ok.")
        {
            i += 1;
            continue;
        }
        keep[i] = true;
        let base = indent_of(line);
        let mut j = i + 1;
        while j < lines.len() {
            let next = lines[j];
            // Source locations and assertion rows are independent signals.
            // Unindented panic messages are also explanations; they cannot be
            // discarded merely because Rust prints them on the next line.
            let t = next.trim_start();
            if !usable(next)
                || is_failure_signal(next)
                || t.starts_with(['✓', '✔', '√'])
                || t.starts_with("PASS ")
            {
                break;
            }
            let explains = LABELLED.is_match(next.trim_start())
                || indent_of(next) > base
                || line.contains("panicked at");
            if !explains {
                break;
            }
            keep[j] = true;
            j += 1;
        }
        i = j.max(i + 1);
    }
    let mut present = std::collections::HashMap::new();
    for line in after.lines() {
        *present
            .entry(SGR.replace_all(line.trim(), "").into_owned())
            .or_insert(0_usize) += 1;
    }
    lines
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(l, _)| *l)
        .filter(|l| {
            let count = present
                .entry(SGR.replace_all(l.trim(), "").into_owned())
                .or_default();
            if *count > 0 {
                *count -= 1;
                false
            } else {
                true
            }
        })
        .map(str::to_string)
        .collect()
}

/// `compressed` plus the failure lines of `original` it lost, and how many
/// were lost. Unchanged when nothing was lost.
pub fn reinject_lost_failure_lines(original: &str, compressed: &str) -> (String, usize) {
    let lost = lost_failure_lines(original, compressed);
    if lost.is_empty() {
        return (compressed.to_string(), 0);
    }
    let mut out = String::with_capacity(compressed.len() + 64 * lost.len());
    out.push_str(compressed);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!(
        "[lm-resizer: {} lignes d'échec omises par la compression, réinjectées ci-dessous]\n",
        lost.len()
    ));
    for line in &lost {
        out.push_str(line);
        out.push('\n');
    }
    (out, lost.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_couleur_ne_cache_ni_assertion_ni_localisation() {
        let colored = "\x1b[31mE   assert 301 == 300\x1b[0m\n\x1b[31mtests/orders.py:42: in test_total\x1b[0m\n";
        assert_eq!(lost_failure_lines(colored, "").len(), 2);
        assert!(lost_failure_lines(
            colored,
            "E   assert 301 == 300\ntests/orders.py:42: in test_total\n"
        )
        .is_empty());
        assert!(lost_failure_lines(
            "[2026-10-02 12:00:00] INFO compile.rs:42 building module\n",
            ""
        )
        .is_empty());
    }

    #[test]
    fn assertions_messages_et_localisations_hors_budget_restent_visibles() {
        for diagnostic in [
            "E   assert 301 == 300",
            "assert total == 300",
            "tests/test_orders.py:42: in test_order_total",
            "called `Option::unwrap()` on a `None` value",
            "called `Result::unwrap()` on an `Err` value: boom",
            "      at tests/orders.test.ts:42:9",
            " ❯ src/foo.test.ts:12:5",
            "The command '/bin/sh -c npm test' returned a non-zero code: 1",
            "CANCELED",
            "npm ERR! code ELIFECYCLE",
            "valueerror: bad input",
            "AssertionError: mismatch",
        ] {
            let original = format!(
                "{}{}\n{}",
                "INFO ok\n".repeat(200),
                diagnostic,
                "INFO ok\n".repeat(200)
            );
            let (view, count) = reinject_lost_failure_lines(&original, "1 failed");
            assert!(count > 0, "{diagnostic}");
            assert!(view.contains(diagnostic.trim()), "{diagnostic}");
        }
        let original =
            "thread 'main' panicked at src/main.rs:10:5:\nmessage libre sans mot clef\n\n";
        let (view, _) =
            reinject_lost_failure_lines(original, "thread 'main' panicked at src/main.rs:10:5:");
        assert!(view.contains("message libre sans mot clef"));
    }

    #[test]
    fn ni_plafond_ni_sous_chaine_ne_masquent_un_diagnostic() {
        let original = (0..350)
            .map(|n| format!("ERROR diagnostic {n}: {}\n", "x".repeat(500)))
            .collect::<String>();
        let (view, count) = reinject_lost_failure_lines(&original, "ERROR diagnostic 123: partial");
        assert_eq!(count, 350);
        for line in original.lines() {
            assert!(view.contains(line));
        }
        assert_eq!(
            lost_failure_lines("ERROR one\nERROR one\n", "ERROR one\n").len(),
            1
        );
        assert_eq!(
            lost_failure_lines("ERROR one\n", "prefix ERROR one suffix\n").len(),
            1
        );
    }

    #[test]
    fn rien_de_perdu_rien_d_ajoute() {
        let t = "ok\n##[error]boom\n";
        assert_eq!(reinject_lost_failure_lines(t, t), (t.to_string(), 0));
    }

    #[test]
    fn les_erreurs_perdues_reviennent_avec_leur_multiplicite() {
        let original = "a\nERROR one\nb\n##[error]two\nERROR one\n";
        let (out, n) = reinject_lost_failure_lines(original, "[3 lines omitted]");
        assert_eq!(n, 3);
        let i1 = out.find("ERROR one").unwrap();
        let i2 = out.find("##[error]two").unwrap();
        assert!(i1 < i2);
        assert_eq!(out.matches("ERROR one").count(), 2);
    }

    #[test]
    fn une_ligne_geante_peut_porter_un_diagnostic() {
        let blob = format!("{{\"error\":null,\"data\":\"{}\"}}", "x".repeat(1000));
        assert_eq!(lost_failure_lines(&blob, ""), vec![blob]);
    }

    #[test]
    fn les_lignes_explicatives_sans_mot_d_echec_suivent_leur_echec() {
        // Forme réelle d'un échec Assert.Contains de xUnit (dotnet test --logger detailed).
        let original = "noise\n  Error Message:\n   Assert.Contains() Failure: Item not found in collection\nCollection: [\"alpha\", \"beta\"]\nNot found:  \"gamma\"\n  Stack Trace:\nnoise suivant\n";
        let (out, _) = reinject_lost_failure_lines(original, "[omitted]");
        assert!(out.contains("Collection: [\"alpha\", \"beta\"]"), "{out}");
        assert!(out.contains("Not found:  \"gamma\""), "{out}");
        assert!(!out.contains("noise suivant"), "{out}");
    }

    #[test]
    fn une_ligne_ordinaire_apres_un_echec_n_est_pas_une_explication() {
        let original = "ERROR disque plein\nINFO nettoyage lancé\nINFO fin\n";
        let lost = lost_failure_lines(original, "");
        assert_eq!(lost, vec!["ERROR disque plein".to_string()]);
    }

    #[test]
    fn un_cadre_de_pile_localise_le_diagnostic() {
        let original = "Error: Stored value failed validation\n    at StorageService.load (/w/src/s.js:12:3)\n    at Array.forEach (<anonymous>)\n";
        assert_eq!(
            lost_failure_lines(original, ""),
            vec![
                "Error: Stored value failed validation".to_string(),
                "    at StorageService.load (/w/src/s.js:12:3)".to_string()
            ]
        );
    }

    #[test]
    fn un_test_reussi_n_est_pas_une_explication() {
        let original =
            "Error: Test timed out in 15000ms.\n   ✓ conserve le français par défaut  3031ms\n";
        assert_eq!(lost_failure_lines(original, "").len(), 1);
    }

    #[test]
    fn une_explication_deja_visible_n_est_pas_dupliquee() {
        let original = "Assert.Contains() Failure\nCollection: [1]\nNot found: 2\n";
        let lost = lost_failure_lines(original, "Assert.Contains() Failure\nCollection: [1]\n");
        assert_eq!(lost, vec!["Not found: 2".to_string()]);
    }

    #[test]
    fn les_lignes_expected_actual_sans_mot_erreur_sont_reinjectees() {
        let original = "noise\nExpected: 19,90 €\nActual:   20,00 €\nPanierTests.cs:line 44\n";
        let (out, n) = reinject_lost_failure_lines(original, "[omitted]");
        assert_eq!(n, 3);
        assert!(out.contains("Expected: 19,90 €"));
        assert!(out.contains("Actual:   20,00 €"));
        assert!(out.contains("PanierTests.cs:line 44"));
    }

    #[test]
    fn les_lignes_js_python_error_sont_reinjectees() {
        let mut original = String::new();
        for _ in 0..50 {
            original.push_str("INFO ok\n");
        }
        original.push_str("AssertionError: expected 3\n");
        original.push_str("INFO ok\n");
        original.push_str("ValueError: bad input\n");

        let (out, n) = reinject_lost_failure_lines(&original, "[omitted]");
        assert_eq!(n, 2);
        assert!(out.contains("AssertionError: expected 3"));
        assert!(out.contains("ValueError: bad input"));
    }

    #[test]
    fn les_lignes_info_errorcode_ne_sont_pas_reinjectees() {
        let original = "INFO ErrorCode=0\nINFO ok\n";
        let lost = lost_failure_lines(original, "");
        assert!(lost.is_empty());
    }
}
