//! Package-manager output views. Only recognised progress is discarded;
//! application output and dependency-resolution failures remain visible.
pub fn filter(name: &str, sub: &str, raw: &str) -> String {
    if name == "pip" && sub == "list" {
        if let Some(view) = inventory(raw) {
            return view;
        }
    }
    let rows: Vec<_> = raw
        .lines()
        .filter(|line| {
            let s = line.trim();
            match name {
                "npm" => {
                    !(s.is_empty()
                        || (line.starts_with('>') && line.contains('@'))
                        || s.starts_with("npm notice")
                        || s.starts_with("Progress: resolved "))
                }
                "pnpm" if matches!(sub, "install" | "add" | "i" | "update") => {
                    !s.starts_with("Progress: resolved ")
                }
                "pip" if sub == "install" => {
                    !(s.starts_with("Downloading ")
                        || s.starts_with("Using cached ")
                        || s.starts_with("Collecting ")
                        || s.starts_with("Requirement already satisfied:"))
                }
                "uv" if matches!(sub, "sync" | "add" | "pip") => !s.starts_with("Downloading "),
                _ => true,
            }
        })
        .collect();
    // Empty output does not prove success. Keep it and let the producer's
    // status carry the result instead of inventing an "ok" message.
    if rows.is_empty() || rows.len() == raw.lines().count() {
        return raw.into();
    }
    let mut view = rows.join("\n");
    view.push('\n');
    view
}

fn inventory(raw: &str) -> Option<String> {
    let items: Vec<serde_json::Value> = serde_json::from_str(raw).ok()?;
    let mut groups = std::collections::BTreeMap::<char, Vec<(&str, &str)>>::new();
    for item in &items {
        let name = item["name"].as_str()?;
        let version = item["version"].as_str()?;
        groups
            .entry(name.chars().next()?.to_ascii_uppercase())
            .or_default()
            .push((name, version));
    }
    if items.is_empty() {
        return Some("pip list: No packages installed".into());
    }
    let mut out = format!("pip list: {} packages\n", items.len());
    for (letter, packages) in groups {
        out.push_str(&format!("\n[{letter}]\n"));
        for (name, version) in packages {
            out.push_str(&format!("  {name} ({version})\n"));
        }
    }
    Some(out.trim_end().into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn npm_diagnostics_keep_indentation_and_operands() {
        let raw =
            "\n> example@1.0 test\n> jest\n\n  Expected: 42\n  Received: 43\nnpm ERR! failed\n";
        assert_eq!(
            filter("npm", "test", raw),
            "> jest\n  Expected: 42\n  Received: 43\nnpm ERR! failed\n"
        );
    }
    #[test]
    fn installs_preserve_unknown_diagnostics() {
        for (name, sub, progress) in [
            ("pnpm", "install", "Progress: resolved 50"),
            ("pip", "install", "Collecting package"),
            ("uv", "sync", "Downloading package"),
        ] {
            let raw = format!("{progress}\nERROR conflict 50%\n  package==1 requires other==2\n");
            let out = filter(name, sub, &raw);
            assert!(out.contains("ERROR conflict 50%\n  package==1 requires other==2"));
            assert!(!out.contains(progress));
        }
    }
    #[test]
    fn inventory_does_not_drop_packages() {
        let raw = r#"[{"name":"a","version":"1"},{"name":"b","version":"02"}]"#;
        let out = inventory(raw).unwrap();
        assert!(out.contains("a (1)"));
        assert!(out.contains("b (02)"));
        assert!(inventory("[{\"name\":\"a\"}]").is_none());
    }
}
