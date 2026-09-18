//! What an outside tool tells us to keep when we shorten a source file.
//!
//! # Why this is not a Code Explorer interface
//!
//! Compressing a source file line by line keeps the beginning of the file,
//! which is rarely the part that matters. Something that understands the code
//! has to say which line ranges carry the meaning.
//!
//! Code Explorer can say that, and it was the obvious first answer. It is the
//! wrong shape for an interface, for two reasons that turned out to matter more
//! than the convenience:
//!
//! - lm-resizer must work on a machine where Code Explorer is not installed,
//!   which is most machines. An interface named after one tool becomes an
//!   interface that only one tool can satisfy.
//! - The machine-readable surface we would need does not exist yet. In the
//!   repository of record at 0.2.1, `--json` is offered by `doctor`,
//!   `hotspots`, `coupling` and `ownership` — and not by `query`, `context`,
//!   `impact` or `analyze`, which are the ones carrying structural meaning.
//!   Waiting for it would block the whole mechanism.
//!
//! So the interface is a plain document: line ranges with weights. `ctags`,
//! tree-sitter, a symbol grep, a person writing it by hand — anything can
//! produce one. Code Explorer becomes the best producer rather than the only
//! one, and the retention machinery can be built and tested today.
//!
//! # The conservative rule
//!
//! Advice only ever *protects* lines. It never marks a line for removal. A tool
//! that fails to mention a range must not cause that range to be dropped: a
//! missing relation in a call graph does not prove the code is unreachable, and
//! Code Explorer's graph is known to miss calls through interfaces and dynamic
//! dispatch. Silence means "I don't know", never "not needed".

use serde::{Deserialize, Serialize};

/// One protected region of a file, with how much it matters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetentionRange {
    /// First line to keep, 1-based and inclusive.
    pub start_line: usize,
    /// Last line to keep, 1-based and inclusive.
    pub end_line: usize,
    /// How much this range matters relative to the others, higher first.
    /// Absent means "matters, unranked", which sorts as 1.0.
    #[serde(default = "poids_par_defaut")]
    pub weight: f64,
    /// Free-form label for diagnostics — a symbol name, usually. Never used to
    /// decide anything.
    #[serde(default)]
    pub label: Option<String>,
}

fn poids_par_defaut() -> f64 {
    1.0
}

/// A whole document of advice, as an advisor produces it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RetentionAdvice {
    /// Who produced this, for diagnostics. Never used to decide anything: a
    /// range from `ctags` is worth exactly as much as one from Code Explorer.
    #[serde(default)]
    pub advisor: Option<String>,
    #[serde(default)]
    pub ranges: Vec<RetentionRange>,
}

impl RetentionAdvice {
    /// Parse advice, rejecting nothing that can be salvaged.
    ///
    /// An advisor is an outside program; its output is data, not instructions,
    /// and it may be partly wrong. A malformed document yields no advice, which
    /// degrades to line-by-line compression — never an error that stops the
    /// caller from compressing at all.
    pub fn from_json(raw: &str) -> Option<Self> {
        let mut advice: Self = serde_json::from_str(raw).ok()?;
        advice.ranges.retain(range_is_usable);
        Some(advice)
    }

    /// True when the advice protects this line.
    pub fn protects(&self, line: usize) -> bool {
        self.ranges
            .iter()
            .any(|r| line >= r.start_line && line <= r.end_line)
    }

    /// Ranges worth keeping, best first, limited to a line budget.
    ///
    /// Returned in file order, because a caller splicing them back into a file
    /// needs them ordered by position, not by importance. Ties break on
    /// position so the result is deterministic: two runs on the same input give
    /// the same output, which is what makes a compression cache worth having.
    pub fn best_within(&self, budget_lines: usize) -> Vec<&RetentionRange> {
        let mut classement: Vec<&RetentionRange> = self.ranges.iter().collect();
        classement.sort_by(|a, b| {
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.start_line.cmp(&b.start_line))
                .then(a.end_line.cmp(&b.end_line))
        });

        let mut retenus: Vec<&RetentionRange> = Vec::new();
        let mut consomme = 0usize;
        for plage in classement {
            let lignes = plage.end_line - plage.start_line + 1;
            if consomme + lignes > budget_lines {
                continue;
            }
            consomme += lignes;
            retenus.push(plage);
        }
        retenus.sort_by(|a, b| {
            a.start_line
                .cmp(&b.start_line)
                .then(a.end_line.cmp(&b.end_line))
        });
        retenus
    }

    /// Total number of distinct lines the advice protects.
    pub fn protected_line_count(&self, total_lines: usize) -> usize {
        (1..=total_lines)
            .filter(|line| self.protects(*line))
            .count()
    }
}

/// A range we can act on: ordered, in-file, and not a weight we cannot compare.
fn range_is_usable(r: &RetentionRange) -> bool {
    r.start_line >= 1 && r.end_line >= r.start_line && r.weight.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_un_document_simple() {
        let advice = RetentionAdvice::from_json(
            r#"{"advisor":"ctags","ranges":[{"start_line":10,"end_line":20,"weight":3.0}]}"#,
        )
        .unwrap();
        assert_eq!(advice.advisor.as_deref(), Some("ctags"));
        assert_eq!(advice.ranges.len(), 1);
        assert!(advice.protects(10));
        assert!(advice.protects(20));
        assert!(!advice.protects(21));
    }

    #[test]
    fn un_poids_absent_vaut_un() {
        let advice =
            RetentionAdvice::from_json(r#"{"ranges":[{"start_line":1,"end_line":2}]}"#).unwrap();
        assert_eq!(advice.ranges[0].weight, 1.0);
    }

    #[test]
    fn les_plages_absurdes_sont_ecartees_sans_tout_rejeter() {
        // Un conseiller peut se tromper sur une plage sans que le reste soit
        // perdu : on écarte l'aberrante et on garde les autres.
        let advice = RetentionAdvice::from_json(
            r#"{"ranges":[
                {"start_line":0,"end_line":5},
                {"start_line":30,"end_line":10},
                {"start_line":7,"end_line":9}
            ]}"#,
        )
        .unwrap();
        assert_eq!(advice.ranges.len(), 1);
        assert_eq!(advice.ranges[0].start_line, 7);
    }

    #[test]
    fn un_document_invalide_ne_donne_pas_de_conseil_et_ne_casse_rien() {
        assert!(RetentionAdvice::from_json("pas du json").is_none());
        assert!(RetentionAdvice::from_json("").is_none());
        // Un conseiller qui ne dit rien est un conseiller silencieux, pas une
        // erreur : on retombe sur la compression ligne à ligne.
        let vide = RetentionAdvice::from_json(r#"{"ranges":[]}"#).unwrap();
        assert!(!vide.protects(1));
        assert_eq!(vide.best_within(100).len(), 0);
    }

    #[test]
    fn le_budget_garde_les_plages_les_plus_lourdes_mais_les_rend_dans_l_ordre_du_fichier() {
        let advice = RetentionAdvice::from_json(
            r#"{"ranges":[
                {"start_line":100,"end_line":104,"weight":9.0,"label":"important"},
                {"start_line":1,"end_line":5,"weight":1.0,"label":"accessoire"},
                {"start_line":50,"end_line":54,"weight":5.0,"label":"moyen"}
            ]}"#,
        )
        .unwrap();

        // Dix lignes de budget : les deux plus lourdes tiennent, la légère non.
        let retenus = advice.best_within(10);
        assert_eq!(retenus.len(), 2);
        // Rendues dans l'ordre du fichier, pas dans celui de l'importance.
        assert_eq!(retenus[0].start_line, 50);
        assert_eq!(retenus[1].start_line, 100);
    }

    #[test]
    fn le_classement_est_deterministe_a_poids_egal() {
        let source = r#"{"ranges":[
            {"start_line":40,"end_line":41,"weight":2.0},
            {"start_line":10,"end_line":11,"weight":2.0},
            {"start_line":25,"end_line":26,"weight":2.0}
        ]}"#;
        let attendu: Vec<usize> = RetentionAdvice::from_json(source)
            .unwrap()
            .best_within(4)
            .iter()
            .map(|r| r.start_line)
            .collect();
        // Deux exécutions identiques doivent rendre le même résultat, sans quoi
        // un cache de compression ne vaut rien.
        for _ in 0..5 {
            let obtenu: Vec<usize> = RetentionAdvice::from_json(source)
                .unwrap()
                .best_within(4)
                .iter()
                .map(|r| r.start_line)
                .collect();
            assert_eq!(obtenu, attendu);
        }
        // À poids égal, ce sont les premières du fichier qui passent.
        assert_eq!(attendu, vec![10, 25]);
    }

    #[test]
    fn un_budget_nul_ne_garde_rien_et_ne_panique_pas() {
        let advice =
            RetentionAdvice::from_json(r#"{"ranges":[{"start_line":1,"end_line":3}]}"#).unwrap();
        assert_eq!(advice.best_within(0).len(), 0);
    }

    #[test]
    fn le_conseil_ne_sert_jamais_a_supprimer() {
        // Une ligne non mentionnée n'est pas condamnée : elle est simplement
        // sans protection. Le silence d'un conseiller veut dire « je ne sais
        // pas », jamais « inutile ».
        let advice =
            RetentionAdvice::from_json(r#"{"ranges":[{"start_line":5,"end_line":6}]}"#).unwrap();
        assert!(!advice.protects(1));
        assert_eq!(advice.protected_line_count(10), 2);
    }

    #[test]
    fn les_plages_qui_se_chevauchent_ne_comptent_pas_deux_fois() {
        let advice = RetentionAdvice::from_json(
            r#"{"ranges":[
                {"start_line":1,"end_line":10},
                {"start_line":5,"end_line":15}
            ]}"#,
        )
        .unwrap();
        assert_eq!(advice.protected_line_count(20), 15);
    }
}
