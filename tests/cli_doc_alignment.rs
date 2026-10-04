//! Contrats que la documentation promet et que le binaire doit tenir.
//! Chaque test reprend une commande rejouée par la recette doc <-> binaire.
use std::path::Path;
use std::process::{Command, Output};

fn run(state: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .args(args)
        .env("HOME", state)
        .env("USERPROFILE", state)
        .env("LM_RESIZER_STATE_DIR", state.join("state"))
        .env_remove("LM_RESIZER_STORE")
        .env_remove("LM_RESIZER_TEE")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    }
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

fn log_text() -> String {
    (0..200)
        .map(|n| format!("INFO 2026-10-04T12:00:00Z service ready connection accepted id={n}\n"))
        .collect()
}

fn compress_hash(state: &Path, text: &str) -> String {
    let out = run(state, &["compress", "--json"], Some(text));
    assert!(out.status.success(), "{out:?}");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    report["cache_keys"][0].as_str().unwrap().to_string()
}

#[test]
fn retrieve_accepts_the_hash_in_every_displayed_form() {
    let dir = tempfile::tempdir().unwrap();
    let text = log_text();
    let hash = compress_hash(dir.path(), &text);
    for form in [
        hash.clone(),
        format!("ccr:{hash}"),
        format!("<<ccr:{hash}>>"),
        format!("[full output: <<ccr:{hash}>>]"),
        format!("hash={hash}"),
    ] {
        let out = run(dir.path(), &["retrieve", &form], None);
        assert!(
            out.status.success(),
            "retrieve {form}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&out.stdout), text, "forme {form}");
    }
    let missing = run(
        dir.path(),
        &["retrieve", "ccr:000000000000000000000000"],
        None,
    );
    assert!(!missing.status.success());
}

#[test]
fn stats_accepts_json_and_keeps_the_default_json_shape() {
    let dir = tempfile::tempdir().unwrap();
    let default = run(dir.path(), &["stats"], None);
    let explicit = run(dir.path(), &["stats", "--json"], None);
    assert!(
        explicit.status.success(),
        "{}",
        String::from_utf8_lossy(&explicit.stderr)
    );
    let a: serde_json::Value = serde_json::from_slice(&default.stdout).unwrap();
    let b: serde_json::Value = serde_json::from_slice(&explicit.stdout).unwrap();
    assert_eq!(a, b);
    let both = run(dir.path(), &["stats", "--json", "--markdown"], None);
    assert!(!both.status.success(), "--json et --markdown s'excluent");
}

#[cfg(unix)]
#[test]
fn exec_raw_on_failure_returns_the_full_output_of_a_compressible_log() {
    let dir = tempfile::tempdir().unwrap();
    let script = "i=0; while [ $i -lt 200 ]; do echo 'INFO 2026-10-04T12:00:00Z service ready connection accepted from client'; i=$((i+1)); done; exit 4";
    let plain = run(dir.path(), &["exec", "--", "sh", "-c", script], None);
    let raw = run(
        dir.path(),
        &["exec", "--raw-on-failure", "--", "sh", "-c", script],
        None,
    );
    let stream = run(
        dir.path(),
        &[
            "exec",
            "--raw-on-failure",
            "--stream",
            "--",
            "sh",
            "-c",
            script,
        ],
        None,
    );
    for out in [&plain, &raw, &stream] {
        assert_eq!(out.status.code(), Some(4));
    }
    let full =
        200 * "INFO 2026-10-04T12:00:00Z service ready connection accepted from client\n".len();
    assert!(plain.stdout.len() < full / 4, "sans option : compresse");
    assert!(
        raw.stdout.len() >= full,
        "--raw-on-failure : {} octets",
        raw.stdout.len()
    );
    assert!(
        stream.stdout.len() >= full,
        "--stream : {} octets",
        stream.stdout.len()
    );
}

fn second_log() -> String {
    (0..150)
        .map(|n| format!("WARN 2026-10-04T13:00:00Z second payload slot={n}\n"))
        .collect()
}

fn mcp_retrieve(state: &Path, shown: &str) -> serde_json::Value {
    let requests = format!(
        "{}\n{}\n",
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"lm_resizer_retrieve","arguments":{"hash":shown}}})
    );
    let out = run(state, &["mcp"], Some(&requests));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|v| v["id"] == 2)
        .unwrap()
}

#[test]
fn retrieve_refuses_ambiguous_or_unrecognised_inputs_on_cli_and_mcp() {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (log_text(), second_log());
    let a = compress_hash(dir.path(), &first);
    let b = compress_hash(dir.path(), &second);
    assert_ne!(a, b);
    let refused = [
        // deux marqueurs : le dernier ne doit jamais gagner en silence
        format!("[full output: <<ccr:{a}>>]\n[full output: <<ccr:{b}>>]"),
        format!("hash={a}\n<<ccr:{b}>>"),
        format!("hash={a}\nhash={b}"),
        // le marqueur ccr: ne doit pas masquer un hash= valide
        format!("[full output: <<ccr:{b}>>]\nRetrieve more: hash={a}]"),
        format!("ccr:hash={a}"),
        format!("hash=ccr:{a}"),
        // prefixes et decorations non documentes
        format!("myhash={a}"),
        format!("notccr:{a}"),
        format!("({a})"),
        // reperes de la vue compress, pas des cles
        "<<ccr:1130452ebc4f,string,3.1KB>>".to_string(),
    ];
    for shown in &refused {
        let out = run(dir.path(), &["retrieve", shown], None);
        assert!(!out.status.success(), "CLI a accepte {shown:?}");
        assert!(out.stdout.is_empty(), "CLI a rendu un corps pour {shown:?}");
        let reply = mcp_retrieve(dir.path(), shown);
        assert!(
            reply["error"].is_object() || reply["result"]["isError"] == true,
            "MCP a accepte {shown:?}: {reply}"
        );
    }
    let ambiguous = run(
        dir.path(),
        &["retrieve", &format!("hash={a}\nhash={b}")],
        None,
    );
    assert!(String::from_utf8_lossy(&ambiguous.stderr).contains("ambiguous"));
    let marker = run(
        dir.path(),
        &["retrieve", "<<ccr:1130452ebc4f,string,3.1KB>>"],
        None,
    );
    assert!(String::from_utf8_lossy(&marker.stderr).contains("not a retrieval key"));
    // les formes valides restent exactes, aussi par MCP
    for shown in [
        a.clone(),
        format!("ccr:{a}"),
        format!("<<ccr:{a}>>"),
        format!("[full output: <<ccr:{a}>>]"),
        format!("hash={a}"),
        format!("hash={a}]"),
    ] {
        let reply = mcp_retrieve(dir.path(), &shown);
        assert!(
            reply.to_string().contains("connection accepted"),
            "MCP {shown:?}: {reply}"
        );
    }
}

#[test]
fn image_max_dimension_below_64_is_refused_with_or_without_output() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("t.png");
    image::RgbImage::from_pixel(80, 80, image::Rgb([200, 30, 30]))
        .save(&png)
        .unwrap();
    let path = png.to_str().unwrap();
    let alone = run(dir.path(), &["image", path, "--max-dimension", "32"], None);
    assert!(!alone.status.success());
    assert!(String::from_utf8_lossy(&alone.stderr).contains("at least 64"));
    let out = dir.path().join("o.png");
    let with = run(
        dir.path(),
        &[
            "image",
            path,
            "--max-dimension",
            "32",
            "--output",
            out.to_str().unwrap(),
        ],
        None,
    );
    assert!(!with.status.success());
    assert!(
        run(dir.path(), &["image", path, "--max-dimension", "64"], None)
            .status
            .success()
    );
}

#[test]
fn retrieve_accepts_the_whole_view_printed_by_compress() {
    let dir = tempfile::tempdir().unwrap();
    let rows: Vec<_> = (0..40)
        .map(|id| serde_json::json!({"id": id, "note": "Z".repeat(800)}))
        .collect();
    let json_doc = serde_json::to_string(&rows).unwrap();
    let log: String = log_text();
    let cases: Vec<(String, Vec<&str>)> = vec![
        (json_doc, vec!["compress"]),
        (log.clone(), vec!["compress", "--token-budget", "80"]),
        (log, vec!["compress"]),
    ];
    for (original, args) in cases {
        let view = run(dir.path(), &args, Some(&original));
        assert!(view.status.success(), "{view:?}");
        let view = String::from_utf8(view.stdout).unwrap();
        assert!(view.len() < original.len(), "{args:?} n'a pas compresse");
        for shown in [view.clone(), view.lines().last().unwrap().to_string()] {
            let out = run(dir.path(), &["retrieve", &shown], None);
            assert!(
                out.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(String::from_utf8_lossy(&out.stdout), original, "{args:?}");
            let reply = mcp_retrieve(dir.path(), &shown);
            assert!(reply["result"].is_object(), "MCP {args:?}: {reply}");
        }
    }
}
