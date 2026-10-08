//! Durcissement du proxy HTTP local (audit du 08/10/2026) : pas de redirection suivie avec la
//! clé d'API, refus d'une requête dont `Host` n'est pas local (rebinding DNS), refus d'une
//! écoute hors boucle locale sans demande explicite.
#![cfg(unix)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const KEY: &str = "sk-test-LMR-PROXY-0123456789";

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

struct Proxy {
    child: Child,
    port: u16,
    _home: tempfile::TempDir,
}

impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_proxy(upstream: &str, provider: &str) -> Proxy {
    let home = tempfile::tempdir().unwrap();
    let port = free_port();
    let child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .args(["serve", "--bind", &format!("127.0.0.1:{port}")])
        .args(["--upstream", upstream, "--provider", provider])
        .env("LM_RESIZER_API_KEY", KEY)
        .env("HOME", home.path())
        .env("LM_RESIZER_STATE_DIR", home.path().join("state"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let proxy = Proxy {
        child,
        port,
        _home: home,
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return proxy;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("le proxy n'écoute pas sur {port}");
}

/// Requête HTTP/1.1 brute ; rend la réponse complète.
fn request(port: u16, head: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let raw = format!(
        "{head}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(raw.as_bytes()).unwrap();
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response
}

/// Serveur à une connexion par requête ; envoie chaque requête reçue (en-têtes) sur un canal.
fn serve(response: impl Fn(&str) -> String + Send + 'static) -> (u16, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buffer = [0u8; 8192];
            let n = stream.read(&mut buffer).unwrap_or(0);
            let seen = String::from_utf8_lossy(&buffer[..n]).into_owned();
            let _ = stream.write_all(response(&seen).as_bytes());
            let _ = tx.send(seen);
        }
    });
    (port, rx)
}

#[test]
fn upstream_redirect_is_not_followed_with_the_api_key() {
    // Cible de la redirection : n'importe quel autre hôte ou port.
    let (steal_port, stolen) =
        serve(|_| "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".into());
    let (upstream_port, _) = serve(move |_| {
        format!(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{steal_port}/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
    });
    let proxy = start_proxy(&format!("http://127.0.0.1:{upstream_port}"), "anthropic");
    let body = r#"{"model":"m","max_tokens":1,"messages":[{"role":"user","content":"hi"}]}"#;
    let _ = request(
        proxy.port,
        &format!(
            "POST /v1/messages HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json",
            proxy.port
        ),
        body,
    );
    if let Ok(seen) = stolen.recv_timeout(Duration::from_secs(2)) {
        panic!("la redirection a été suivie, la cible a reçu :\n{seen}");
    }
}

#[test]
fn request_with_a_foreign_host_header_is_refused() {
    let proxy = start_proxy("http://127.0.0.1:9", "openai");
    let port = proxy.port;
    let ok = request(
        port,
        &format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}"),
        "",
    );
    assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
    let localhost = request(
        port,
        &format!("GET /health HTTP/1.1\r\nHost: localhost:{port}"),
        "",
    );
    assert!(localhost.starts_with("HTTP/1.1 200"), "{localhost}");
    let foreign = request(port, "GET /health HTTP/1.1\r\nHost: evil.example", "");
    assert!(
        foreign.starts_with("HTTP/1.1 421") || foreign.starts_with("HTTP/1.1 403"),
        "{foreign}"
    );
    let foreign_stats = request(
        port,
        &format!("GET /stats HTTP/1.1\r\nHost: evil.example:{port}"),
        "",
    );
    assert!(
        !foreign_stats.starts_with("HTTP/1.1 200"),
        "{foreign_stats}"
    );
}

#[test]
fn non_loopback_listen_requires_an_explicit_flag() {
    let home = tempfile::tempdir().unwrap();
    let port = free_port();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .args(["serve", "--bind", &format!("0.0.0.0:{port}")])
        .env("HOME", home.path())
        .env("LM_RESIZER_STATE_DIR", home.path().join("state"))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Un serveur qui accepterait l'adresse tournerait indéfiniment : borner l'attente.
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            break None;
        }
        thread::sleep(Duration::from_millis(50));
    };
    let mut stderr = String::new();
    let _ = child.stderr.take().unwrap().read_to_string(&mut stderr);
    let _ = child.wait();
    assert!(
        status.is_some_and(|s| !s.success()),
        "le proxy a accepté d'écouter hors boucle locale : {stderr}"
    );
    assert!(stderr.contains("--allow-non-loopback"), "{stderr}");
    // Rien n'écoute après le refus.
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_err());
}
