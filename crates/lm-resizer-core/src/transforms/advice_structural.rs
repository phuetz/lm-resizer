//! Body elision guided by an outside advisor whose advice was verified.
//!
//! [`crate::transforms::retention_advice`] only ever *protects* lines. That is
//! the right rule for line-by-line compression, and it is useless for source
//! code in practice: a producer that reports every symbol reports the namespace
//! and the class too, whose ranges cover the whole file, so protecting them
//! protects everything. Measured on the first real Code Explorer output: a C#
//! file came out byte for byte identical.
//!
//! What a structural producer actually knows is where each *callable* starts
//! and ends. This module uses exactly that and nothing more:
//!
//! - advice must be [`AdviceFreshness::Fresh`] — hashed on the very bytes in
//!   hand — or nothing happens;
//! - only ranges whose `kind` names a callable (`Method`, `Function`,
//!   `Constructor`, ...) may lose lines, and only their *body*: the lines
//!   strictly between the brace that opens the body and the brace that closes
//!   the range. Signatures, attributes, doc comments, imports, type and
//!   namespace declarations are kept verbatim;
//! - the body brace is found by matching the range's final `}` against the
//!   brace lexer's stack, not by taking the first `{` in the range — a
//!   destructured parameter or an initializer in a signature opens a brace too;
//! - a range whose braces do not balance inside its own lines, as lexed here, is
//!   not touched: the producer and this lexer disagree, and disagreement is
//!   resolved by keeping the text;
//! - kept lines are copied with their original line endings, so a CRLF file
//!   stays CRLF where it is shown;
//! - the original goes to the CCR store, and every marker names its key.
//!
//! Ranges the query names are kept whole: asking about `Calcul` and receiving
//! `Calcul`'s signature without its body would answer a different question.

use crate::ccr::{compute_key, CcrStore};
use crate::transforms::retention_advice::{AdviceFreshness, RetentionAdvice, RetentionRange};

/// Kinds whose body may be elided. Anything else — classes, namespaces,
/// properties, unknown kinds — is kept.
const CALLABLE_KINDS: &[&str] = &["Function", "Method", "Constructor", "ControllerAction"];

/// Why the advice did or did not change the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdviceStatus {
    /// Bodies were elided; the output is shorter and the original is stored.
    Applied,
    /// The advisor hashed different bytes. Its line numbers describe another
    /// file.
    Stale,
    /// The advice carries no hash, so nothing ties it to these bytes.
    Unknown,
    /// No range says which lines are callables. Protect-only advice.
    NoCallableKinds,
    /// Language unknown or without a brace-delimited body syntax handled here.
    UnsupportedLanguage,
    /// Advice was valid but eliding would not shorten anything.
    NoGain,
}

impl AdviceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Stale => "stale",
            Self::Unknown => "unknown-freshness",
            Self::NoCallableKinds => "no-callable-kinds",
            Self::UnsupportedLanguage => "unsupported-language",
            Self::NoGain => "no-gain",
        }
    }
}

/// Outcome of [`elide_bodies_with_advice`]. `output` is the input itself
/// unless `status` is [`AdviceStatus::Applied`].
#[derive(Debug, Clone, PartialEq)]
pub struct AdviceElision {
    pub status: AdviceStatus,
    pub output: String,
    pub elided_bodies: usize,
    pub elided_lines: usize,
    /// Callable ranges left whole because their braces did not balance.
    pub guard_rejects: usize,
    /// Labels kept whole because the query named them.
    pub focused: Vec<String>,
    pub language: Option<String>,
    pub ccr_key: Option<String>,
}

impl AdviceElision {
    fn unchanged(input: &str, status: AdviceStatus, language: Option<String>) -> Self {
        Self {
            status,
            output: input.to_string(),
            elided_bodies: 0,
            elided_lines: 0,
            guard_rejects: 0,
            focused: Vec::new(),
            language,
            ccr_key: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    /// C#, Java, C, C++, Go, Kotlin, Swift, Rust.
    CLike,
    /// JavaScript, TypeScript, PHP: `'...'` is a string, not a character.
    Script,
}

fn family_of(language: &str) -> Option<Family> {
    match language.to_ascii_lowercase().as_str() {
        "csharp" | "cs" | "c#" | "java" | "c" | "cpp" | "c++" | "go" | "kotlin" | "swift"
        | "rust" | "rs" => Some(Family::CLike),
        "javascript" | "js" | "typescript" | "ts" | "tsx" | "jsx" | "php" => Some(Family::Script),
        _ => None,
    }
}

fn language_from_path(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "cs" => "csharp",
        "java" => "java",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "cpp",
        "go" => "go",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "rs" => "rust",
        "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "ts" | "mts" | "cts" | "tsx" => "typescript",
        "php" => "php",
        "py" => "python",
        "rb" => "ruby",
        _ => return None,
    })
}

/// Elide callable bodies named by `advice`, if and only if the advice was
/// computed from these exact bytes.
pub fn elide_bodies_with_advice(
    input: &str,
    advice: &RetentionAdvice,
    query: &str,
    store: Option<&dyn CcrStore>,
) -> AdviceElision {
    let language = advice.language.clone().or_else(|| {
        advice
            .source_path
            .as_deref()
            .and_then(language_from_path)
            .map(str::to_string)
    });

    match advice.freshness(input) {
        AdviceFreshness::Fresh => {}
        AdviceFreshness::Stale => {
            return AdviceElision::unchanged(input, AdviceStatus::Stale, language)
        }
        AdviceFreshness::Unknown => {
            return AdviceElision::unchanged(input, AdviceStatus::Unknown, language)
        }
    }

    let Some(family) = language.as_deref().and_then(family_of) else {
        return AdviceElision::unchanged(input, AdviceStatus::UnsupportedLanguage, language);
    };

    let lines: Vec<&str> = input.split_inclusive('\n').collect();
    let callables: Vec<&RetentionRange> = advice
        .ranges
        .iter()
        .filter(|r| {
            r.kind
                .as_deref()
                .is_some_and(|k| CALLABLE_KINDS.contains(&k))
                && r.start_line >= 1
                && r.end_line <= lines.len()
                && r.end_line > r.start_line
        })
        .collect();
    if callables.is_empty() {
        return AdviceElision::unchanged(input, AdviceStatus::NoCallableKinds, language);
    }

    let terms = focus_terms(query);
    let braces = lex_braces(&lines, family);

    // Outermost first, so a local function inside an elided method is gone
    // with it and never gets a second marker.
    let mut ordered = callables;
    ordered.sort_by(|a, b| {
        a.start_line
            .cmp(&b.start_line)
            .then(b.end_line.cmp(&a.end_line))
    });

    let mut elisions: Vec<(usize, usize, String)> = Vec::new(); // body lines, 1-based inclusive
    let mut kept_whole_until = 0usize;
    let mut focused = Vec::new();
    let mut guard_rejects = 0usize;
    for range in ordered {
        if range.start_line <= kept_whole_until {
            continue;
        }
        if elisions
            .last()
            .is_some_and(|(_, end, _)| range.start_line <= *end + 1)
        {
            continue;
        }
        let label = range.label.clone().unwrap_or_default();
        if is_focused(&label, &terms) {
            focused.push(label);
            kept_whole_until = range.end_line;
            continue;
        }
        let Some(open_line) = body_open_line(&braces, range.start_line, range.end_line) else {
            guard_rejects += 1;
            continue;
        };
        let (first, last) = (open_line + 1, range.end_line - 1);
        // One line replaced by one marker saves nothing and hides something.
        if last < first || last - first + 1 < 2 {
            continue;
        }
        elisions.push((first, last, label));
    }

    if elisions.is_empty() {
        let mut out = AdviceElision::unchanged(input, AdviceStatus::NoGain, language);
        out.guard_rejects = guard_rejects;
        out.focused = focused;
        return out;
    }

    let key = compute_key(input.as_bytes());
    let eol = if input.contains("\r\n") { "\r\n" } else { "\n" };
    let elided_lines: usize = elisions.iter().map(|(a, b, _)| b - a + 1).sum();

    let mut out = String::with_capacity(input.len() / 2);
    let mut next = elisions.iter().peekable();
    let mut line_no = 1usize;
    let mut banner_done = false;
    while line_no <= lines.len() {
        let raw = lines[line_no - 1];
        if !banner_done && !(line_no == 1 && keeps_first_line(raw)) {
            out.push_str(&format!(
                "// [lm-resizer: structure guidée par {} (sha256 vérifié) ; {} corps omis, {} lignes ; original complet : lm-resizer retrieve {}]{}",
                advice.advisor.as_deref().unwrap_or("un conseiller externe"),
                elisions.len(),
                elided_lines,
                key,
                eol
            ));
            banner_done = true;
        }
        if let Some((first, last, label)) = next.peek() {
            if line_no == *first {
                let indent: String = raw
                    .chars()
                    .take_while(|c| c.is_whitespace() && *c != '\r' && *c != '\n')
                    .collect();
                out.push_str(&format!(
                    "{indent}/* lm-resizer : {} lignes omises (corps de {}) — lm-resizer retrieve {} */{eol}",
                    last - first + 1,
                    label.replace("*/", "* /"),
                    key
                ));
                line_no = *last + 1;
                next.next();
                continue;
            }
        }
        out.push_str(raw);
        line_no += 1;
    }

    if out.len() >= input.len() {
        let mut unchanged = AdviceElision::unchanged(input, AdviceStatus::NoGain, language);
        unchanged.guard_rejects = guard_rejects;
        unchanged.focused = focused;
        return unchanged;
    }

    if let Some(s) = store {
        s.put(&key, input);
    }

    AdviceElision {
        status: AdviceStatus::Applied,
        output: out,
        elided_bodies: elisions.len(),
        elided_lines,
        guard_rejects,
        focused,
        language,
        ccr_key: Some(key),
    }
}

/// A shebang, a BOM or a PHP opening tag must stay first.
fn keeps_first_line(raw: &str) -> bool {
    raw.starts_with("#!") || raw.starts_with('\u{feff}') || raw.trim_start().starts_with("<?php")
}

fn focus_terms(query: &str) -> Vec<String> {
    query
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| t.chars().count() >= 2)
        .map(|t| t.to_lowercase())
        .collect()
}

fn is_focused(label: &str, terms: &[String]) -> bool {
    if terms.is_empty() || label.is_empty() {
        return false;
    }
    let l = label.to_lowercase();
    terms
        .iter()
        .any(|t| l == *t || (t.chars().count() >= 4 && l.contains(t.as_str())))
}

/// One brace event: 1-based line, `true` for `{`.
type BraceEvent = (usize, bool);

/// The line of the brace matched by the range's last `}`, when the braces
/// inside the range balance and that `}` sits on the range's last line.
fn body_open_line(events: &[BraceEvent], start: usize, end: usize) -> Option<usize> {
    let mut stack: Vec<usize> = Vec::new();
    let mut last_match: Option<(usize, usize)> = None; // (open line, close line)
    for &(line, open) in events.iter().filter(|(l, _)| *l >= start && *l <= end) {
        if open {
            stack.push(line);
        } else {
            let opened = stack.pop()?; // a close with nothing open: not our block
            last_match = Some((opened, line));
        }
    }
    if !stack.is_empty() {
        return None;
    }
    let (open_line, close_line) = last_match?;
    // Opened and closed on the last line (`}); ... {}`): not a body.
    (close_line == end && open_line < end).then_some(open_line)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Code,
    BlockComment,
    /// C# `@"..."`, `""` escapes a quote, may span lines.
    Verbatim,
    /// `"""..."""` (C# raw, Java text block, Kotlin, Swift), may span lines.
    Triple,
    /// `` `...` `` (JS/TS template, Go raw string), may span lines.
    Backtick,
    /// Rust `r"..."`, `r#"..."#`, `br##"..."##`: no escapes, closes on a quote
    /// followed by as many `#` as opened. May span lines.
    RustRaw(usize),
}

/// Braces that are code, not text: strings, characters and comments skipped.
///
/// This lexer is deliberately modest. It exists to *confirm* a producer that
/// parsed the file with a real grammar; where the two disagree the range is
/// kept whole, so a lexing mistake costs a compression opportunity, never text.
fn lex_braces(lines: &[&str], family: Family) -> Vec<BraceEvent> {
    let mut events = Vec::new();
    let mut mode = Mode::Code;
    for (idx, raw) in lines.iter().enumerate() {
        let line_no = idx + 1;
        let chars: Vec<char> = raw.chars().collect();
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();
            match mode {
                Mode::BlockComment => {
                    if c == '*' && next == Some('/') {
                        mode = Mode::Code;
                        i += 2;
                        continue;
                    }
                }
                Mode::Verbatim => {
                    if c == '"' {
                        if next == Some('"') {
                            i += 2;
                            continue;
                        }
                        mode = Mode::Code;
                    }
                }
                Mode::Triple => {
                    if c == '"' && next == Some('"') && chars.get(i + 2) == Some(&'"') {
                        mode = Mode::Code;
                        i += 3;
                        continue;
                    }
                }
                Mode::RustRaw(hashes) => {
                    if c == '"' && (1..=hashes).all(|k| chars.get(i + k) == Some(&'#')) {
                        mode = Mode::Code;
                        i += 1 + hashes;
                        continue;
                    }
                }
                Mode::Backtick => {
                    if c == '\\' {
                        i += 2;
                        continue;
                    }
                    if c == '`' {
                        mode = Mode::Code;
                    }
                }
                Mode::Code => match c {
                    '/' if next == Some('/') => break,
                    '/' if next == Some('*') => {
                        mode = Mode::BlockComment;
                        i += 2;
                        continue;
                    }
                    '#' if family == Family::Script && raw.trim_start().starts_with('#') => break,
                    'r' | 'b' if family == Family::CLike && rust_raw_start(&chars, i).is_some() => {
                        let (hashes, len) = rust_raw_start(&chars, i).unwrap_or((0, 1));
                        mode = Mode::RustRaw(hashes);
                        i += len;
                        continue;
                    }
                    '@' if next == Some('"') => {
                        mode = Mode::Verbatim;
                        i += 2;
                        continue;
                    }
                    '$' if next == Some('@') && chars.get(i + 2) == Some(&'"') => {
                        mode = Mode::Verbatim;
                        i += 3;
                        continue;
                    }
                    '"' if next == Some('"') && chars.get(i + 2) == Some(&'"') => {
                        mode = Mode::Triple;
                        i += 3;
                        continue;
                    }
                    '"' => {
                        i = skip_quoted(&chars, i + 1, '"');
                        continue;
                    }
                    '\'' => {
                        i = match family {
                            Family::Script => skip_quoted(&chars, i + 1, '\''),
                            Family::CLike => skip_char_literal(&chars, i),
                        };
                        continue;
                    }
                    '`' => mode = Mode::Backtick,
                    '{' => events.push((line_no, true)),
                    '}' => events.push((line_no, false)),
                    _ => {}
                },
            }
            i += 1;
        }
    }
    events
}

/// `r"`, `r#"`, `br##"` at `i`, not preceded by an identifier character:
/// returns (number of `#`, length of the opener).
fn rust_raw_start(chars: &[char], i: usize) -> Option<(usize, usize)> {
    if i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
        return None;
    }
    let mut j = i;
    if chars.get(j) == Some(&'b') {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    (chars.get(j) == Some(&'"')).then_some((hashes, j + 1 - i))
}

/// Index just past the closing `quote`, honouring backslash escapes. An
/// unterminated string ends the line.
fn skip_quoted(chars: &[char], mut i: usize, quote: char) -> usize {
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            c if c == quote => return i + 1,
            '\n' => return i,
            _ => i += 1,
        }
    }
    i
}

/// `'x'`, `'\n'`, `'\u{1F600}'` are characters; Rust's `'a` lifetime is not.
fn skip_char_literal(chars: &[char], i: usize) -> usize {
    let rest = &chars[i + 1..];
    if rest.first() == Some(&'\\') {
        if let Some(pos) = rest.iter().skip(1).position(|c| *c == '\'') {
            return i + 1 + 1 + pos + 1;
        }
        return i + 1;
    }
    if rest.len() >= 2 && rest[1] == '\'' {
        return i + 3;
    }
    i + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ccr::InMemoryCcrStore;
    use crate::transforms::retention_advice::sha256_hex_public;

    /// Assez de lignes pour que l'omission paie la bannière et le marqueur :
    /// sinon la porte de non-croissance rend l'original, à juste titre.
    fn remplissage(indent: &str, eol: &str, n: usize) -> String {
        (0..n)
            .map(|i| {
                format!("{indent}trace(\"ligne {i} du corps, assez longue pour compter\");{eol}")
            })
            .collect()
    }

    fn advice_for(
        source: &str,
        language: &str,
        ranges: &[(usize, usize, &str, &str)],
    ) -> RetentionAdvice {
        RetentionAdvice {
            advisor: Some("test".into()),
            source_sha256: Some(sha256_hex_public(source.as_bytes())),
            source_path: None,
            language: Some(language.into()),
            schema: None,
            ranges: ranges
                .iter()
                .map(|(s, e, label, kind)| RetentionRange {
                    start_line: *s,
                    end_line: *e,
                    weight: 1.0,
                    label: Some((*label).into()),
                    kind: Some((*kind).into()),
                })
                .collect(),
        }
    }

    fn cs() -> String {
        format!(
            "using System;\r\n\r\nnamespace Demo\r\n{{\r\n    public class Boite\r\n    {{\r\n        [Fact]\r\n        public void Methode()\r\n        {{\r\n            var s = \"h\u{e9}llo {{\";\r\n            Console.WriteLine(s);\r\n{}        }}\r\n    }}\r\n}}\r\n",
            remplissage("            ", "\r\n", 10)
        )
    }
    // Lignes : 1 using, 3 namespace, 5 class, 7 [Fact], 8 signature, 9 {,
    // 10..21 corps (12 lignes), 22 }, 23 } classe, 24 } namespace.

    fn cs_advice() -> RetentionAdvice {
        advice_for(
            &cs(),
            "csharp",
            &[
                (1, 1, "using System;", "Dependency"),
                (3, 24, "Demo", "Namespace"),
                (5, 23, "Boite", "Class"),
                (7, 22, "Methode", "Method"),
            ],
        )
    }

    #[test]
    fn un_corps_csharp_crlf_est_omis_et_le_reste_garde_ses_octets() {
        let store = InMemoryCcrStore::new();
        let cs = cs();
        let out = elide_bodies_with_advice(&cs, &cs_advice(), "", Some(&store));
        assert_eq!(out.status, AdviceStatus::Applied, "{out:?}");
        assert_eq!(out.elided_bodies, 1);
        assert_eq!(out.elided_lines, 12);
        for kept in [
            "using System;\r\n",
            "namespace Demo\r\n",
            "        [Fact]\r\n",
            "        public void Methode()\r\n",
            "        {\r\n",
            "        }\r\n",
        ] {
            assert!(
                out.output.contains(kept),
                "{kept:?} absent de {:?}",
                out.output
            );
        }
        assert!(!out.output.contains("WriteLine"));
        // L'accolade dans la chaîne ne compte pas : sinon le corps ne fermerait pas.
        assert!(out.output.len() < cs.len());
        assert!(
            !out.output.replace("\r\n", "").contains('\n'),
            "fin de ligne LF introduite"
        );
        let key = out.ccr_key.unwrap();
        assert_eq!(
            store.get(&key).as_deref(),
            Some(cs.as_str()),
            "original octet pour octet"
        );
    }

    #[test]
    fn un_conseil_perime_ou_sans_empreinte_ne_touche_a_rien() {
        let cs = cs();
        let mut advice = cs_advice();
        let modifie = cs.replace("Methode", "Methode2");
        let out = elide_bodies_with_advice(&modifie, &advice, "", None);
        assert_eq!(out.status, AdviceStatus::Stale);
        assert_eq!(out.output, modifie);
        advice.source_sha256 = None;
        let out = elide_bodies_with_advice(&cs, &advice, "", None);
        assert_eq!(out.status, AdviceStatus::Unknown);
        assert_eq!(out.output, cs);
    }

    #[test]
    fn sans_nature_de_plage_rien_n_est_omis() {
        let mut advice = cs_advice();
        for r in &mut advice.ranges {
            r.kind = None;
        }
        let out = elide_bodies_with_advice(&cs(), &advice, "", None);
        assert_eq!(out.status, AdviceStatus::NoCallableKinds);
        assert_eq!(out.output, cs());
    }

    #[test]
    fn la_requete_garde_le_corps_nomme() {
        let out = elide_bodies_with_advice(&cs(), &cs_advice(), "pourquoi Methode échoue", None);
        assert_eq!(out.status, AdviceStatus::NoGain);
        assert_eq!(out.focused, vec!["Methode".to_string()]);
        assert_eq!(out.output, cs());
    }

    #[test]
    fn une_plage_decalee_d_une_ligne_est_laissee_entiere() {
        // L'index a été construit avant l'ajout d'une ligne : la plage finit
        // une ligne trop tôt, sur `Console.WriteLine`.
        let advice = advice_for(&cs(), "csharp", &[(7, 21, "Methode", "Method")]);
        let out = elide_bodies_with_advice(&cs(), &advice, "", None);
        assert_eq!(out.status, AdviceStatus::NoGain);
        assert_eq!(out.guard_rejects, 1);
        assert_eq!(out.output, cs());
    }

    #[test]
    fn un_parametre_destructure_n_est_pas_pris_pour_le_corps() {
        let ts = format!("export function rendre({{\n  a,\n  b,\n}}: Props) {{\n  const x = a + b;\n  return x * 2;\n{}}}\n", remplissage("  ", "\n", 10));
        let advice = advice_for(&ts, "typescript", &[(1, 17, "rendre", "Function")]);
        let out = elide_bodies_with_advice(&ts, &advice, "", None);
        assert_eq!(out.status, AdviceStatus::Applied, "{out:?}");
        assert!(
            out.output.contains("  a,\n  b,\n}: Props) {\n"),
            "{}",
            out.output
        );
        assert!(!out.output.contains("return x"));
        assert_eq!(out.elided_lines, 12);
    }

    #[test]
    fn une_fonction_locale_part_avec_son_parent_sans_second_marqueur() {
        let rs = format!(
            "pub fn externe() {{\n    fn interne() {{\n{}    }}\n    interne();\n}}\n",
            remplissage("        ", "\n", 10)
        );
        let advice = advice_for(
            &rs,
            "rust",
            &[
                (1, 15, "externe", "Function"),
                (2, 13, "interne", "Function"),
            ],
        );
        let out = elide_bodies_with_advice(&rs, &advice, "", None);
        assert_eq!(out.status, AdviceStatus::Applied);
        assert_eq!(out.elided_bodies, 1);
        assert_eq!(out.output.matches("lignes omises").count(), 1);
        assert!(out.output.contains("pub fn externe() {\n"));
    }

    #[test]
    fn chaines_verbatim_brutes_et_caracteres_n_equilibrent_pas_faussement() {
        let cs = format!("class C\n{{\n    string M()\n    {{\n        var a = @\"{{\"\"\n}}\";\n        var b = \"\"\"\n  }}}}}}\n  \"\"\";\n        char c = '{{';\n{}        return a;\n    }}\n}}\n", remplissage("        ", "\n", 10));
        let advice = advice_for(&cs, "csharp", &[(3, 22, "M", "Method")]);
        let out = elide_bodies_with_advice(&cs, &advice, "", None);
        assert_eq!(out.status, AdviceStatus::Applied, "{out:?}");
        assert_eq!(out.elided_lines, 17);
    }

    #[test]
    fn une_duree_de_vie_rust_n_est_pas_un_caractere() {
        let rs = format!(
            "fn f<'a>(x: &'a str) -> &'a str {{\n    let y = '}}';\n{}    let z = x;\n    z\n}}\n",
            remplissage("    ", "\n", 10)
        );
        let advice = advice_for(&rs, "rust", &[(1, 15, "f", "Function")]);
        let out = elide_bodies_with_advice(&rs, &advice, "", None);
        assert_eq!(out.status, AdviceStatus::Applied, "{out:?}");
        assert_eq!(out.elided_lines, 13);
    }

    #[test]
    fn une_chaine_brute_rust_n_ouvre_pas_de_bloc() {
        let rs = format!("fn t() {{\n    let j = r#\"{{\"ranges\":[{{\"a\":1}}\"#;\n    let k = br\"}}\";\n    let var = 1;\n{}}}\n", remplissage("    ", "\n", 10));
        let advice = advice_for(&rs, "rust", &[(1, 15, "t", "Function")]);
        let out = elide_bodies_with_advice(&rs, &advice, "", None);
        assert_eq!(out.status, AdviceStatus::Applied, "{out:?}");
        assert_eq!(out.guard_rejects, 0);
        assert_eq!(out.elided_lines, 13);
    }

    #[test]
    fn python_n_est_pas_traite_ici() {
        let py = "def f():\n    a = 1\n    b = 2\n    return a\n";
        let advice = advice_for(py, "python", &[(1, 4, "f", "Function")]);
        let out = elide_bodies_with_advice(py, &advice, "", None);
        assert_eq!(out.status, AdviceStatus::UnsupportedLanguage);
        assert_eq!(out.output, py);
    }
}
