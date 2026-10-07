//! Container tables and logs. No inferred severity or zero-error claims.
pub fn table(raw: &str) -> String {
    let mut lines = raw.lines();
    let Some(header) = lines.next() else {
        return raw.into();
    };
    if !(header.starts_with("NAME ")
        || header.starts_with("NAMESPACE ")
        || header.starts_with("CONTAINER ID "))
    {
        return raw.into();
    }
    // Two or more spaces delimit display columns. A single space inside a
    // status, command or age belongs to the cell and remains untouched.
    let columns = regex::Regex::new(r" {2,}").unwrap();
    let width = columns.split(header.trim_end()).count();
    if width < 2 {
        return raw.into();
    }
    let mut out = vec![columns
        .split(header.trim_end())
        .collect::<Vec<_>>()
        .join(" | ")];
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells: Vec<_> = columns.split(line.trim_end()).collect();
        if cells.len() != width {
            return raw.into();
        }
        out.push(cells.join(" | "));
    }
    out.join("\n") + "\n"
}

pub fn logs(raw: &str) -> String {
    // Only exact adjacent repetitions are folded; a unique stderr failure is
    // never hidden because its wording lacks an ERROR prefix.
    if raw.contains("[repeat ") || !raw.ends_with('\n') {
        return raw.into();
    }
    let lines: Vec<_> = raw.lines().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let mut end = i + 1;
        while end < lines.len() && lines[end] == lines[i] {
            end += 1;
        }
        out.push_str(lines[i]);
        out.push('\n');
        if end - i > 1 {
            out.push_str(&format!("[repeat {} more]\n", end - i - 1));
        }
        i = end;
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_and_names_are_never_rewritten() {
        let raw="NAME      READY   STATUS             RESTARTS   AGE\ninvoice   0/1     CrashLoopBackOff   12         3d\n";
        let out = table(raw);
        assert!(out.contains("invoice | 0/1 | CrashLoopBackOff | 12 | 3d"));
        assert_eq!(
            table("Error from server: denied\n"),
            "Error from server: denied\n"
        );
    }
    #[test]
    fn a_failure_without_level_survives_logs() {
        let raw = "checkpoint\ncat: cannot open /missing\ncat: cannot open /missing\n";
        let out = logs(raw);
        assert!(out.contains("cat: cannot open /missing\n[repeat 1 more]"));
        assert!(!out.contains("0 errors"));
    }
}
