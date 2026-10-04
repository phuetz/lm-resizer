//! Text folds with a checked inverse. Preserve order, spelling and every row.
use anyhow::{bail, Context, Result};

pub fn fold(raw: &str, search: bool) -> Option<String> {
    if !raw.ends_with('\n') || raw.contains('\r') || raw.lines().count() < 5 {
        return None;
    }
    let mut out = if search { "Matches v1\n" } else { "Paths v1\n" }.to_owned();
    let mut previous = "";
    for line in raw.lines() {
        let (prefix, suffix) = if search {
            // Locate the numeric field, rather than treating a drive letter's
            // colon as the end of the filename. Never normalize the path.
            let (file, tail) = line.match_indices(':').find_map(|(offset, _)| {
                let file = &line[..offset];
                let tail = &line[offset + 1..];
                let (number, _) = tail.split_once(':')?;
                (file.contains(['/', '\\'])
                    && !file.contains('\t')
                    && !number.starts_with('0')
                    && !number.is_empty()
                    && number.bytes().all(|b| b.is_ascii_digit()))
                .then_some((file, tail))
            })?;
            (file, tail)
        } else {
            if (line.ends_with(':') || line.contains('\t')) || line.starts_with([' ', '@']) {
                return None;
            }
            let Some(split) = line.rfind(['/', '\\']).map(|n| n + 1) else {
                if line.is_empty() {
                    return None;
                }
                out.push('=');
                out.push_str(line);
                out.push('\n');
                continue;
            };
            let (dir, name) = line.split_at(split);
            if name.is_empty() {
                out.push('=');
                out.push_str(line);
                out.push('\n');
                continue;
            }
            (dir, name)
        };
        if prefix != previous {
            out.push('@');
            out.push_str(prefix);
            out.push('\n');
            previous = prefix;
        }
        out.push(' ');
        out.push_str(suffix);
        out.push('\n');
    }
    (expand(&out).ok()? == raw).then_some(out)
}

pub fn expand(view: &str) -> Result<String> {
    let (search, body) = if let Some(s) = view.strip_prefix("Matches v1\n") {
        (true, s)
    } else if let Some(s) = view.strip_prefix("Paths v1\n") {
        (false, s)
    } else {
        return expand_runs(view);
    };
    let mut prefix = None;
    let mut out = String::new();
    for line in body.lines() {
        if let Some(p) = line.strip_prefix('@') {
            prefix = Some(p);
        } else if let Some(s) = line.strip_prefix(' ') {
            out.push_str(prefix.context("folded row without a prefix")?);
            if search {
                out.push(':');
            }
            out.push_str(s);
            out.push('\n');
        } else if let Some(literal) = line.strip_prefix('=') {
            out.push_str(literal);
            out.push('\n');
        } else {
            bail!("invalid folded row");
        }
    }
    Ok(out)
}

pub fn expand_runs(view: &str) -> Result<String> {
    let mut out = String::new();
    let mut previous = None;
    for line in view.split_inclusive('\n') {
        if let Some(n) = line
            .strip_prefix("[repeat ")
            .and_then(|s| s.strip_suffix(" more]\n"))
        {
            let n: usize = n.parse().context("invalid run length")?;
            let p: &str = previous.context("repeat without a preceding row")?;
            anyhow::ensure!(
                n.checked_mul(p.len())
                    .and_then(|n| n.checked_add(out.len()))
                    .is_some_and(|n| n <= 512 * 1024 * 1024),
                "expanded view too large"
            );
            for _ in 0..n {
                out.push_str(p);
            }
        } else {
            out.push_str(line);
            previous = Some(line);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_preserve_spaces_order_and_revisited_directories() {
        let raw = "src/module/a\nsrc/module/b c\nother/place/d\nother/place/e\nsrc/module/f\n";
        assert_eq!(expand(&fold(raw, false).unwrap()).unwrap(), raw);
        for input in [
            "dir:\na\nb\nc\nd\n",
            "src/a\n",
            "src/a\nsrc/b\nsrc/c\nsrc/d\nsrc/e",
        ] {
            assert!(fold(input, false).is_none());
        }
    }
    #[test]
    fn matches_preserve_all_hits_and_colons() {
        let raw = (1..=20)
            .map(|n| format!("src/long/path/file.rs:{n}:value: detail\n"))
            .collect::<String>();
        assert_eq!(expand(&fold(&raw, true).unwrap()).unwrap(), raw);
        assert!(fold(&"2026-09-02 14:30:00 [FATAL] x\n".repeat(20), true).is_none());
    }
    #[test]
    fn drive_letters_spaces_and_network_paths_keep_exact_spelling() {
        for prefix in [
            r"C:\projet été\source avec espace",
            r"\\serveur\partage\source",
            "/src/with spaces",
        ] {
            let paths = (1..=20)
                .map(|n| format!("{prefix}\\file{n}.rs\n"))
                .collect::<String>();
            assert_eq!(expand(&fold(&paths, false).unwrap()).unwrap(), paths);
            let matches = (1..=20)
                .map(|n| format!("{prefix}\\file.rs:{n}:value:detail\n"))
                .collect::<String>();
            assert_eq!(expand(&fold(&matches, true).unwrap()).unwrap(), matches);
        }
        assert!(fold(&"C:\\src\\file.rs:00:bad\n".repeat(20), true).is_none());
    }
    #[test]
    fn runs_have_an_exact_inverse_and_a_bounded_decoder() {
        assert_eq!(
            expand_runs("INFO a\n[repeat 4 more]\nERROR last\n").unwrap(),
            "INFO a\n".repeat(5) + "ERROR last\n"
        );
        assert!(expand_runs("INFO a\n[repeat 999999999999999 more]\n").is_err());
    }
}
