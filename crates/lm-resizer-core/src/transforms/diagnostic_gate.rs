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
//! text no longer shows are appended, short and in their original order, under
//! a marker. One definition of "failure line" serves every path.

use regex::Regex;
use std::sync::LazyLock;

/// Lines that report a failure.
pub static FAILURE_SIGNAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)##\[error\]|\berror\b|\bfail(ed|ure|s)?\b|\bFAIL\b|✘|✗|×|exception|panic|traceback|assertion|\b(expected|actual|received)\b|:line\s+[0-9]+|\bvalues differ\b|\bdiff(erence)?\b|exit code [1-9]|timed? ?out",
    )
    .expect("valid failure regex")
});

/// Longest line considered: a minified blob that happens to contain "error"
/// is not a diagnostic line.
const MAX_LINE: usize = 400;
/// At most this many lines are re-injected; beyond, head and tail are kept.
const MAX_REINJECTED: usize = 200;

/// Lines that follow a failure line and explain it, at most this many.
const EXPLANATION_WINDOW: usize = 4;

/// `Label: value` — how test frameworks print what they compared:
/// `Collection: ["alpha", "beta"]`, `Not found: "gamma"`, `Locator: …`,
/// `Call log:`. Such a line carries no failure word, and a word list cannot
/// enumerate every framework's labels; its position after a failure line is
/// what marks it as an explanation.
static LABELLED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\[[^\]]*\]\s*)?[A-Za-z][A-Za-z ]{0,28}:(\s|$)").expect("valid label regex")
});

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Failure lines of `before` that `after` no longer shows, each with the
/// explanation lines that follow it in `before` and that `after` lost too.
/// Trimmed, deduplicated, in the original order.
///
/// An explanation line comes right after a failure line (or after another
/// explanation line), before any blank line, and is either labelled
/// (`Collection: …`) or indented deeper than the failure line. Measured on a
/// real `dotnet test --logger detailed`: extending the failure word list with
/// `expected|actual` (review fix of 23/09) left `Collection:` and `Not found:`
/// lost on the generic paths; the window recovers them without a new word.
pub fn lost_failure_lines(before: &str, after: &str) -> Vec<String> {
    let lines: Vec<&str> = before.lines().collect();
    let mut keep = vec![false; lines.len()];
    let usable = |l: &str| !l.trim().is_empty() && l.len() <= MAX_LINE;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if !(usable(line) && FAILURE_SIGNAL.is_match(line)) {
            i += 1;
            continue;
        }
        if !after.contains(line.trim()) {
            keep[i] = true;
        }
        let base = indent_of(line);
        let mut j = i + 1;
        while j < lines.len() && j <= i + EXPLANATION_WINDOW {
            let next = lines[j];
            // Stack frames are not explanations: measured on a real CI log,
            // 56 of 91 re-injected lines were `at …` frames printed by passing
            // tests' console.error. The frames that locate a failure are
            // failure lines already (`:line N`, `❯ file:line:col`).
            // Nor is a passing test's line (`✓ …`), measured as noise after a
            // timeout line in a real Vitest log.
            let t = next.trim_start();
            if !usable(next)
                || FAILURE_SIGNAL.is_match(next)
                || t.starts_with("at ")
                || t.starts_with(['✓', '✔', '√'])
                || t.starts_with("PASS ")
            {
                break;
            }
            let explains = LABELLED.is_match(next.trim_start()) || indent_of(next) > base;
            if !explains {
                break;
            }
            if !after.contains(next.trim()) {
                keep[j] = true;
            }
            j += 1;
        }
        i = j.max(i + 1);
    }
    let mut seen = std::collections::HashSet::new();
    lines
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|(l, _)| l.trim())
        .filter(|l| seen.insert(l.to_string()))
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
    let mut out = String::with_capacity(compressed.len() + 64 * lost.len().min(MAX_REINJECTED));
    out.push_str(compressed);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!(
        "[lm-resizer: {} lignes d'échec omises par la compression, réinjectées ci-dessous]\n",
        lost.len()
    ));
    let half = MAX_REINJECTED / 2;
    if lost.len() > MAX_REINJECTED {
        for l in &lost[..half] {
            out.push_str(l);
            out.push('\n');
        }
        out.push_str(&format!(
            "[... {} lignes d'échec de plus ...]\n",
            lost.len() - MAX_REINJECTED
        ));
        for l in &lost[lost.len() - half..] {
            out.push_str(l);
            out.push('\n');
        }
    } else {
        for l in &lost {
            out.push_str(l);
            out.push('\n');
        }
    }
    (out, lost.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rien_de_perdu_rien_d_ajoute() {
        let t = "ok\n##[error]boom\n";
        assert_eq!(reinject_lost_failure_lines(t, t), (t.to_string(), 0));
    }

    #[test]
    fn les_erreurs_perdues_reviennent_dans_l_ordre_sans_doublon() {
        let original = "a\nERROR one\nb\n##[error]two\nERROR one\n";
        let (out, n) = reinject_lost_failure_lines(original, "[3 lines omitted]");
        assert_eq!(n, 2);
        let i1 = out.find("ERROR one").unwrap();
        let i2 = out.find("##[error]two").unwrap();
        assert!(i1 < i2);
        assert_eq!(out.matches("ERROR one").count(), 1);
    }

    #[test]
    fn une_ligne_geante_n_est_pas_un_diagnostic() {
        let blob = format!("{{\"error\":null,\"data\":\"{}\"}}", "x".repeat(1000));
        assert!(lost_failure_lines(&blob, "").is_empty());
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
    fn un_cadre_de_pile_n_est_pas_une_explication() {
        let original = "Error: Stored value failed validation\n    at StorageService.load (/w/src/s.js:12:3)\n    at Array.forEach (<anonymous>)\n";
        assert_eq!(
            lost_failure_lines(original, ""),
            vec!["Error: Stored value failed validation".to_string()]
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
}
