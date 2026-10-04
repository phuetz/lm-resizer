//! Reversible patch presentation. Factor repeated transport headers and equal
//! consecutive rows; never select, cap, or omit source/context/commit records.
use anyhow::{bail, Context, Result};

const MAGIC: &str = "Patch v2\n";
const LEGACY: &str = "Patch v1\n";

fn literal(row: &str) -> String {
    if row.starts_with(['\\'])
        || ["F ", "FT ", "File ", "FileT ", "Repeat ", "End "]
            .iter()
            .any(|p| row.starts_with(p))
    {
        format!("\\{row}\n")
    } else {
        format!("{row}\n")
    }
}

fn header(rows: &[&str]) -> Option<String> {
    let [transport, index, before, after, ..] = rows else {
        return None;
    };
    let field = before.strip_prefix("--- a/")?;
    let (name, separator) = field.strip_suffix('\t').map_or((field, ""), |s| (s, "\t"));
    // Recognition is equality of complete producer records, not path parsing.
    // Quoted, renamed, binary and unusual headers are still ordinary literals.
    if name.contains('\r')
        || *transport != format!("diff --git a/{name} b/{name}")
        || *after != format!("+++ b/{name}{separator}")
    {
        return None;
    }
    let version = index.strip_prefix("index ")?;
    if version.contains('\r') {
        return None;
    }
    // A tab separates the path from the object pair. The common regular-file
    // mode belongs to the format, not to a guessed property of the filename.
    if !name.contains('\t') {
        if let Some(objects) = version.strip_suffix(" 100644") {
            return Some(format!(
                "{} {name}\t{objects}\n",
                if separator.is_empty() { "F" } else { "FT" }
            ));
        }
    }
    Some(format!(
        "{} {} {version}\n",
        if separator.is_empty() {
            "File"
        } else {
            "FileT"
        },
        serde_json::to_string(name).ok()?
    ))
}

pub fn render(raw: &str) -> String {
    if raw.is_empty() || (raw.starts_with(MAGIC) || raw.starts_with(LEGACY)) {
        return raw.into();
    }
    let rows: Vec<_> = raw.split_terminator('\n').collect();
    let mut encoded = String::from(MAGIC);
    let mut cursor = 0;
    while cursor < rows.len() {
        if let Some(record) = header(&rows[cursor..]) {
            encoded.push_str(&record);
            cursor += 4;
            continue;
        }
        let row = rows[cursor];
        let count = rows[cursor..].iter().take_while(|s| **s == row).count();
        let repetition = format!("Repeat {}\n", count - 1);
        let plain = literal(row);
        encoded.push_str(&plain);
        if count > 1 && repetition.len() < plain.len() * (count - 1) {
            encoded.push_str(&repetition);
        } else {
            for _ in 1..count {
                encoded.push_str(&plain);
            }
        }
        cursor += count;
    }
    encoded.push_str(if raw.ends_with('\n') {
        "End 1\n"
    } else {
        "End 0\n"
    });
    if encoded.len() < raw.len()
        && crate::token_metrics::TokenCounts::measure(raw, &encoded).tokens_saved > 0
    {
        encoded
    } else {
        raw.into()
    }
}

pub fn expand(view: &str) -> Result<String> {
    let modern = view.starts_with(MAGIC);
    let body = view
        .strip_prefix(MAGIC)
        .or_else(|| view.strip_prefix(LEGACY))
        .context("missing patch format header")?;
    let mut out = String::new();
    let mut previous = String::new();
    let mut finished = false;
    for row in body.split_terminator('\n') {
        if finished {
            let hint = row
                .strip_prefix("[tee:")
                .or_else(|| row.strip_prefix("[raw: "))
                .and_then(|s| s.strip_suffix(']'));
            anyhow::ensure!(
                row.is_empty()
                    || hint.is_some_and(
                        |id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit())
                    ),
                "data after patch terminator"
            );
            continue;
        }
        if matches!(row, "End 0" | "End 1") {
            if row == "End 0" {
                anyhow::ensure!(out.pop() == Some('\n'), "missing final row");
            }
            finished = true;
            continue;
        }
        let chunk = if let Some(text) = row.strip_prefix('\\') {
            format!("{text}\n")
        } else if modern && (row.starts_with("F ") || row.starts_with("FT ")) {
            let (name, objects) = row
                .split_once(' ')
                .unwrap()
                .1
                .split_once('\t')
                .context("missing patch object pair")?;
            anyhow::ensure!(!name.contains(['\r', '\n']), "multiline file name");
            let separator = if row.starts_with("FT ") { "\t" } else { "" };
            format!("diff --git a/{name} b/{name}\nindex {objects} 100644\n--- a/{name}{separator}\n+++ b/{name}{separator}\n")
        } else if let Some(fields) = row
            .strip_prefix("File ")
            .or_else(|| row.strip_prefix("FileT "))
        {
            let mut values = serde_json::Deserializer::from_str(fields).into_iter::<String>();
            let name = values.next().context("missing file name")??;
            anyhow::ensure!(!name.contains(['\r', '\n']), "multiline file name");
            let version = fields[values.byte_offset()..]
                .strip_prefix(' ')
                .context("missing file version")?;
            let separator = if row.starts_with("FileT ") { "\t" } else { "" };
            format!("diff --git a/{name} b/{name}\nindex {version}\n--- a/{name}{separator}\n+++ b/{name}{separator}\n")
        } else if let Some(count) = row.strip_prefix("Repeat ") {
            let n: usize = count.parse().context("invalid repetition")?;
            anyhow::ensure!(!previous.is_empty(), "repetition without a row");
            let bytes = n
                .checked_mul(previous.len())
                .context("repetition overflow")?;
            anyhow::ensure!(
                bytes <= (512usize * 1024 * 1024).saturating_sub(out.len()),
                "patch expansion too large"
            );
            previous.repeat(n)
        } else if row.starts_with("End ") {
            bail!("invalid patch terminator");
        } else {
            format!("{row}\n")
        };
        anyhow::ensure!(
            out.len() + chunk.len() <= 512 * 1024 * 1024,
            "patch expansion too large"
        );
        if let Some(last) = chunk.split_inclusive('\n').next_back() {
            previous = last.into();
        }
        out.push_str(&chunk);
    }
    anyhow::ensure!(finished, "missing patch terminator");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inverse(raw: &str) {
        let view = render(raw);
        let decoded = if view.starts_with(MAGIC) {
            expand(&view).unwrap()
        } else {
            view.clone()
        };
        assert_eq!(decoded, raw);
        if view != raw {
            assert!(crate::token_metrics::TokenCounts::measure(raw, &view).tokens_saved > 0);
        }
    }
    #[test]
    fn repetition_preserves_every_source_row_and_both_newline_styles() {
        for ending in ["\n", "\r\n"] {
            for final_newline in [true, false] {
                let mut raw = format!("commit abc{ending}Author: Someone{ending}diff --git a/x b/x{ending}@@ -0,0 +1,400 @@{ending}");
                raw.push_str(&format!("+important changed line{ending}").repeat(400));
                raw.push_str("tail sentinel");
                if final_newline {
                    raw.push_str(ending);
                }
                inverse(&raw);
                assert_ne!(render(&raw), raw);
            }
        }
    }
    #[test]
    fn headers_context_and_marker_like_source_have_an_exact_inverse() {
        for path in ["a b/c", "folder b/file", "été.rs", "quote\"file"] {
            let raw = format!("commit xyz\ndiff --git a/{path} b/{path}\nindex aaa..bbb 100644\n--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n{}", " contextual line\n".repeat(50));
            inverse(&raw);
        }
        let raw = format!(
            "{}File accidental\nRepeat 999\nEnd 1\n\\literal\n",
            "some long unchanged row\n".repeat(40)
        );
        inverse(&raw);
    }
    #[test]
    fn distinct_large_patches_keep_all_context_and_changes() {
        let mut raw = String::new();
        for commit in 0..3 {
            raw.push_str(&format!(
                "commit {commit}\nAuthor: Author {commit} <a{commit}@example.test>\n\n"
            ));
            let path = "a long directory/source file with spaces.rs";
            raw.push_str(&format!("diff --git a/{path} b/{path}\nindex abc..def 100644\n--- a/{path}\n+++ b/{path}\n@@ -1,350 +1,350 @@ source\n"));
            for row in 0..50 {
                raw.push_str(&format!(" unchanged {row}\n"));
            }
            for row in 0..300 {
                raw.push_str(&format!("-old {row}\n+new {row}\n"));
            }
            raw.push_str("\\ No newline at end of file\n");
        }
        let view = render(&raw);
        assert_ne!(view, raw);
        assert_eq!(expand(&view).unwrap(), raw);
        assert!(view.contains("+new 299"));
        assert!(view.contains(" unchanged 49"));
    }
    #[test]
    fn compact_headers_preserve_modes_tabs_and_legacy_literals() {
        assert_eq!(
            expand("Patch v1\nF literal\nEnd 1\n").unwrap(),
            "F literal\n"
        );
        for mode in ["100644", "100755", "120000"] {
            for separator in ["", "\t"] {
                let raw = format!("diff --git a/folder/file name b/folder/file name\nindex aaa..bbb {mode}\n--- a/folder/file name{separator}\n+++ b/folder/file name{separator}\n@@ -1 +1 @@\n-old\n+new\n{}F literal\nFT literal\n", " long context\n".repeat(50));
                let view = render(&raw);
                assert!(view.starts_with(MAGIC));
                assert_eq!(expand(&view).unwrap(), raw);
                assert!(view.contains(if mode == "100644" {
                    "\taaa..bbb\n"
                } else {
                    mode
                }));
            }
        }
    }
    #[test]
    fn decoder_rejects_corruption_and_unbounded_expansion() {
        for raw in [
            "Patch v1\nRepeat 2\nEnd 1\n",
            "Patch v1\nx\n",
            "Patch v1\nx\nRepeat 999999999999999999\nEnd 1\n",
            "Patch v1\nx\nEnd 1\nlost text\n",
        ] {
            assert!(expand(raw).is_err());
        }
    }
}
