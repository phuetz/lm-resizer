//! Dedicated, conservative filters for additional command families not covered by the
//! older native routes. Every removed line is a recognized progress decoration.
//! Failures, locations, identities and result rows are retained verbatim.

pub fn filter(command: &[String], raw: &str) -> Option<(&'static str, String)> {
    let words: Vec<String> = command
        .iter()
        .map(|s| {
            s.rsplit(['/', '\\'])
                .next()
                .unwrap_or(s)
                .to_ascii_lowercase()
        })
        .collect();
    let first = words.first()?.as_str();
    let second = words.get(1).map(String::as_str).unwrap_or("");
    let third = words.get(2).map(String::as_str).unwrap_or("");
    if is_prisma_migrate(command) {
        return Some(("prisma-migrate", apply(raw, Rule::PrismaMigrate)));
    }
    let (name, rule) = match (first, second, third) {
        ("git", "stash", "list") => ("git-stash-list", Rule::Rows),
        ("cat" | "head" | "tail", _, _) => ("file-read", Rule::Identity),
        ("cargo", "nextest", _) => ("cargo-nextest", Rule::Nextest),
        ("rubocop", _, _) | ("bundle", "exec", "rubocop") => ("rubocop", Rule::Rubocop),
        ("rake", _, _) | ("bundle", "exec", "rake") => ("rake", Rule::Rake),
        ("docker" | "podman", "images", _) | ("docker" | "podman", "image", "ls") => {
            return Some(("docker-images", filter_docker_images(command, raw)));
        }
        ("docker" | "podman", "compose", "up" | "down" | "ps" | "pull" | "restart")
        | ("docker-compose", "up" | "down" | "ps" | "pull" | "restart", _) => {
            ("docker-compose", Rule::Compose)
        }
        ("ast-grep" | "sg", _, _) => {
            if command
                .iter()
                .any(|arg| arg == "--json" || arg.starts_with("--json=") || arg == "--format=json")
            {
                return Some(("ast-grep-json", raw.to_string()));
            }
            ("ast-grep", Rule::Search)
        }
        ("psql", _, _) => ("psql", Rule::Psql),
        ("gt", "log" | "status", _) => ("graphite", Rule::Rows),
        ("pnpm", "list" | "outdated", _) => ("pnpm-list", Rule::Rows),
        ("golangci-lint", "run", _) => ("golangci-lint", Rule::GoLint),
        ("go", "build", _) => ("go-build", Rule::GoBuild),
        ("curl", _, _) => ("curl", Rule::Curl),
        ("aws", _, _) => {
            let table = command
                .windows(2)
                .any(|pair| pair[0] == "--output" && pair[1].eq_ignore_ascii_case("table"))
                || command
                    .iter()
                    .any(|arg| arg.eq_ignore_ascii_case("--output=table"));
            (
                "aws",
                if table {
                    Rule::AwsTable
                } else {
                    Rule::Identity
                },
            )
        }
        ("wc", _, _) => ("wc", Rule::Rows),
        _ => return None,
    };
    if matches!(rule, Rule::Identity) {
        return Some((name, raw.to_string()));
    }
    Some((name, apply(raw, rule)))
}

#[derive(Clone, Copy)]
enum Rule {
    Nextest,
    Rubocop,
    Rake,
    Rows,
    Compose,
    Search,
    Psql,
    GoBuild,
    GoLint,
    Curl,
    AwsTable,
    PrismaMigrate,
    Identity,
}

pub fn is_prisma_migrate(command: &[String]) -> bool {
    let words = command
        .iter()
        .map(|s| {
            s.rsplit(['/', '\\'])
                .next()
                .unwrap_or(s)
                .to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    let args = words.iter().map(String::as_str).collect::<Vec<_>>();
    matches!(
        args.as_slice(),
        ["prisma", "migrate", ..]
            | ["npx" | "bunx" | "pnpm" | "yarn", "prisma", "migrate", ..]
            | ["npx", "--yes" | "-y", "prisma", "migrate", ..]
            | ["npm" | "pnpm" | "yarn", "exec", "prisma", "migrate", ..]
    )
}

fn apply(raw: &str, rule: Rule) -> String {
    let mut out = Vec::new();
    let mut omitted = 0usize;
    for line in raw.lines() {
        let trim = line.trim();
        let skip = match rule {
            Rule::Nextest => trim.starts_with("PASS [") || trim.starts_with("START ["),
            Rule::Rubocop => {
                trim.starts_with("Inspecting ")
                    || (trim.len() >= 8 && trim.chars().all(|c| matches!(c, '.' | 'C' | 'W')))
            }
            Rule::Rake => trim.starts_with("** Invoke ") || trim.starts_with("** Execute "),
            Rule::Rows => false,
            Rule::Identity => false,
            Rule::Search => trim.is_empty(),
            Rule::Compose => {
                trim.starts_with("[+] Running ")
                    || trim.starts_with("[+] Building ")
                    || trim.starts_with("[+] Pulling ")
            }
            Rule::Psql => {
                !trim.is_empty()
                    && trim.contains('+')
                    && trim.chars().all(|c| matches!(c, '-' | '+' | ' '))
            }
            Rule::GoBuild => trim.starts_with("go: downloading "),
            Rule::GoLint => trim.starts_with("level=info msg=\"golangci-lint has version "),
            Rule::Curl => {
                if trim.starts_with("% Total    % Received")
                    || trim.starts_with("Dload  Upload   Total")
                {
                    true
                } else {
                    let segments: Vec<&str> =
                        trim.split('\r').filter(|s| !s.trim().is_empty()).collect();
                    if segments.is_empty() {
                        false
                    } else {
                        segments.into_iter().all(|seg| {
                            let fields: Vec<&str> = seg.split_whitespace().collect();
                            if fields.len() < 10 {
                                return false;
                            }
                            let mut has_time = false;
                            for field in &fields {
                                let is_time = field.contains(':')
                                    && field
                                        .chars()
                                        .all(|c| c.is_ascii_digit() || c == ':' || c == '-');
                                let is_numeric = field.chars().all(|c| {
                                    c.is_ascii_digit()
                                        || matches!(
                                            c,
                                            '.' | ','
                                                | 'k'
                                                | 'K'
                                                | 'm'
                                                | 'M'
                                                | 'g'
                                                | 'G'
                                                | 't'
                                                | 'T'
                                                | 'p'
                                                | 'P'
                                        )
                                });
                                if is_time {
                                    has_time = true;
                                } else if !is_numeric {
                                    return false;
                                }
                            }
                            has_time && !fields.last().unwrap().contains(':')
                        })
                    }
                }
            }
            Rule::AwsTable => {
                trim.len() > 2
                    && trim.contains('-')
                    && trim.chars().all(|ch| matches!(ch, '-' | '+' | '|'))
            }
            Rule::PrismaMigrate => {
                trim.starts_with("Environment variables loaded from ")
                    || trim.starts_with("Prisma CLI Version : ")
            }
        };
        if skip {
            omitted += 1;
            continue;
        }
        // Blank runs add no diagnostic context. One blank remains as a block
        // separator, including in psql and multi-match ast-grep output.
        if trim.is_empty() && out.last().is_some_and(|last: &&str| last.trim().is_empty()) {
            omitted += 1;
            continue;
        }
        out.push(line);
    }
    if omitted == 0 {
        return raw.to_string();
    }
    let mut result = out.join("\n");
    if raw.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn filter_docker_images(command: &[String], raw: &str) -> String {
    // Custom formats and digest/quiet modes do not have Docker's table shape.
    if command.iter().any(|arg| {
        matches!(arg.as_str(), "--format" | "--digests" | "-q" | "--quiet")
            || arg.starts_with("--format=")
    }) {
        return raw.to_string();
    }
    let mut lines = raw.lines();
    let Some(header) = lines.next() else {
        return raw.to_string();
    };
    if header.split_whitespace().collect::<Vec<_>>()
        != ["REPOSITORY", "TAG", "IMAGE", "ID", "CREATED", "SIZE"]
    {
        return raw.to_string();
    }
    let mut compact = vec!["REPOSITORY:TAG IMAGE ID SIZE".to_string()];
    for line in lines {
        let columns = line.split_whitespace().collect::<Vec<_>>();
        if columns.len() < 7 || columns[3..columns.len() - 1].is_empty() {
            return raw.to_string();
        }
        compact.push(format!(
            "{}:{} {} {}",
            columns[0],
            columns[1],
            columns[2],
            columns[columns.len() - 1]
        ));
    }
    let mut output = compact.join("\n");
    if raw.ends_with('\n') {
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::filter;

    fn run(command: &[&str], raw: &str, oracle: &[&str]) -> String {
        let command: Vec<String> = command.iter().map(|s| (*s).to_string()).collect();
        let (_, output) = filter(&command, raw).expect("dedicated filter");
        for fact in oracle {
            assert!(output.contains(fact), "lost {fact:?} from {command:?}");
        }
        assert!(output.len() <= raw.len());
        output
    }

    #[test]
    fn nextest_keeps_failure_test_path_and_line() {
        let raw = "START [ 0.01s] app::passes\nPASS [ 0.02s] app::passes\nFAIL [ 0.03s] app::fails\n---- app::fails stdout ----\npanicked at src/lib.rs:42:9\nassertion failed\nSummary [ 0.04s] 1 passed, 1 failed\n";
        let out = run(
            &["cargo", "nextest", "run"],
            raw,
            &["app::fails", "src/lib.rs:42:9", "1 failed"],
        );
        assert!(!out.contains("app::passes"));
    }

    #[test]
    fn ruby_docker_and_compose_keep_diagnostics() {
        let rubocop = "Inspecting 120 files\nCCCCCCCCCCCC\napp/models/user.rb:17:4: C: Style/GuardClause: Use a guard clause.\n120 files inspected, 1 offense detected\n";
        run(
            &["rubocop"],
            rubocop,
            &["app/models/user.rb:17:4", "1 offense detected"],
        );
        let rake = "** Invoke test (first_time)\n** Execute test\nRun options: --seed 18\nFailure: UserTest#save [test/user_test.rb:19]\nExpected true, got false\n";
        run(
            &["rake", "test"],
            rake,
            &["UserTest#save", "test/user_test.rb:19", "Expected true"],
        );
        let images = "REPOSITORY TAG IMAGE ID CREATED SIZE\napp latest abc123 2 days ago 120MB\n";
        run(
            &["docker", "images"],
            images,
            &["app:latest abc123", "120MB"],
        );
        let compose = "[+] Running 2/2\n ✔ Container db Started\n ✘ Container api Error\napi | Error at src/main.cs:42\n";
        run(
            &["docker", "compose", "up"],
            compose,
            &["db Started", "api Error", "src/main.cs:42"],
        );
    }

    #[test]
    fn search_psql_and_other_rows_keep_all_facts() {
        let search = "src/lib.rs:42:9: foo()\n\nsrc/main.rs:8:2: foo()\n";
        run(
            &["ast-grep", "run"],
            search,
            &["src/lib.rs:42:9", "src/main.rs:8:2"],
        );
        let psql = " id | name\n----+------\n 1 | Alice\n(1 row)\n";
        run(
            &["psql", "-c", "select"],
            psql,
            &["id | name", "1 | Alice", "(1 row)"],
        );
        let go = "go: downloading example.com/module v1.2.3\n./main.go:17:2: undefined: Missing\n";
        run(
            &["go", "build"],
            go,
            &["./main.go:17:2", "undefined: Missing"],
        );
        let curl = "  % Total    % Received\nDload  Upload   Total\nHTTP/1.1 500 Internal Server Error\nX-Request-ID: abc123\nbody error\n";
        run(
            &["curl", "-i"],
            curl,
            &["HTTP/1.1 500", "X-Request-ID: abc123", "body error"],
        );
    }

    #[test]
    fn curl_progress_rows_are_removed_but_body_and_headers_kept() {
        let raw = "  % Total    % Received % Xferd  Average Speed   Time    Time     Time  Current\n                                 Dload  Upload   Total   Spent    Left  Speed\n100     2  100     2    0     0  54054      0 --:--:-- --:--:-- --:--:-- 54054\nHTTP/1.1 500 Internal Server Error\nX-Request-ID: abc123\nbody error\n";
        let out = run(
            &["curl", "-i", "http://x"],
            raw,
            &["HTTP/1.1 500", "X-Request-ID: abc123", "body error"],
        );
        assert!(!out.contains("--:--:--"));

        let raw2 = "\r  0     0    0     0    0     0      0      0 --:--:-- --:--:-- --:--:--     0\r100     2  100     2    0     0  54054      0 --:--:-- --:--:-- --:--:-- 54054\nHTTP/1.1 500 Internal Server Error\nX-Request-ID: abc123\nbody error\n";
        let out2 = run(
            &["curl", "-i", "http://x"],
            raw2,
            &["HTTP/1.1 500", "X-Request-ID: abc123", "body error"],
        );
        assert!(!out2.contains("--:--:--"));
        assert!(!out2.contains("54054"));

        let raw3 = "100 200 300\n";
        let out3 = run(&["curl", "-i", "http://x"], raw3, &["100 200 300"]);
        assert_eq!(out3.trim(), "100 200 300");
    }

    #[test]
    fn ast_grep_json_is_byte_for_byte_passthrough() {
        let raw = "[\n  {\"file\": \"src/lib.rs\", \"line\": 42}\n]\n";
        let command = ["ast-grep", "run", "--json"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(
            filter(&command, raw),
            Some(("ast-grep-json", raw.to_string()))
        );
    }

    #[test]
    fn aws_table_removes_only_borders_and_preserves_every_field() {
        let raw = "------------------------------\n|        ListFunctions       |\n+----------------------------+\n||         Functions        ||\n|+--------------+-----------+|\n|| FunctionName |  Runtime  ||\n|+--------------+-----------+|\n|| sample-api   | python3.12||\n|| sample-job   | nodejs20.x||\n|+--------------+-----------+|\n";
        let out = run(
            &["aws", "lambda", "list-functions", "--output", "table"],
            raw,
            &[
                "FunctionName",
                "Runtime",
                "sample-api",
                "python3.12",
                "sample-job",
                "nodejs20.x",
            ],
        );
        assert!(!out.contains("+--------------+"));
        let text = run(
            &["aws", "lambda", "list-functions", "--output", "text"],
            "sample-api\tpython3.12\n",
            &["sample-api", "python3.12"],
        );
        assert_eq!(text, "sample-api\tpython3.12\n");
    }

    #[test]
    fn prisma_migrate_keeps_applied_names_and_failed_migration() {
        let success = "Environment variables loaded from .env\nPrisma schema loaded from prisma/schema.prisma\n2 migrations found in prisma/migrations\nApplying migration `20260928_add_orders`\nThe following migration(s) have been applied:\n\n  migrations/20260928_add_orders/migration.sql\n\nAll migrations have been successfully applied.\nPrisma CLI Version : 5.15.0\n";
        let filtered = run(
            &["npx", "prisma", "migrate", "deploy"],
            success,
            &[
                "prisma/schema.prisma",
                "20260928_add_orders",
                "migrations/20260928_add_orders/migration.sql",
                "All migrations have been successfully applied",
            ],
        );
        assert!(!filtered.contains("Environment variables loaded"));
        let failure = "Environment variables loaded from .env\nError: P3009\nThe migrate found failed migrations in the target database.\nMigration name: 20260928_add_orders\nDatabase error: relation orders already exists\n";
        run(
            &["prisma", "migrate", "deploy"],
            failure,
            &[
                "P3009",
                "20260928_add_orders",
                "relation orders already exists",
            ],
        );
    }

    #[test]
    fn docker_images_keeps_identity_and_size_on_standard_table_only() {
        let raw = "REPOSITORY TAG IMAGE ID CREATED SIZE\nservice latest abc123 2 days ago 120MB\nworker stable def456 3 weeks ago 90MB\n";
        let out = run(
            &["docker", "images"],
            raw,
            &["service:latest abc123 120MB", "worker:stable def456 90MB"],
        );
        assert!(!out.contains("days ago"));
        let formatted = ["docker", "images", "--format", "{{.ID}}"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(filter(&formatted, raw).unwrap().1, raw);
        let inspect = ["docker", "image", "inspect"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(filter(&inspect, raw).is_none());
    }

    #[test]
    fn golangci_lint_skips_version_banner_but_keeps_issues() {
        let raw = "level=info msg=\"golangci-lint has version 1.59.0\"\nsrc/main.go:42:4: unchecked error (errcheck)\n2 issues found\n";
        let out = run(
            &["golangci-lint", "run"],
            raw,
            &["src/main.go:42:4", "errcheck", "2 issues found"],
        );
        assert!(!out.contains("level=info"));
    }

    #[test]
    fn file_read_keeps_arbitrary_lines_verbatim() {
        let raw = "first\n\n\nsecond\n";
        for program in ["cat", "head", "tail"] {
            let command = vec![program.to_string(), "file.txt".to_string()];
            assert_eq!(filter(&command, raw), Some(("file-read", raw.to_string())));
        }
    }
}
