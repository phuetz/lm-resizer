use axum::http::StatusCode;
use reqwest::Client;
use std::net::TcpListener;
use std::process::Command;

fn get_free_port() -> Option<u16> {
    match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => Some(listener.local_addr().unwrap().port()),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            eprintln!("socket test skipped: bind denied by sandbox");
            None
        }
        Err(error) => panic!("could not bind test socket: {error}"),
    }
}

#[tokio::test]
async fn auth_bind_public_without_token_fails() {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_lm-resizer"));
    let output = cmd
        .arg("serve")
        .arg("--bind")
        .arg("0.0.0.0:0")
        .env_clear()
        .env("HOME", "/tmp")
        .env("USERPROFILE", "/tmp")
        .env("LM_RESIZER_STATE_DIR", "/tmp")
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Refus de démarrer"));
}

#[tokio::test]
async fn auth_routes() {
    let Some(port) = get_free_port() else { return };

    // Test server sans token
    let mut child1 = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .arg("serve")
        .arg("--bind")
        .arg(format!("127.0.0.1:{}", port))
        .env_clear()
        .env("HOME", "/tmp")
        .env("USERPROFILE", "/tmp")
        .env("LM_RESIZER_STATE_DIR", "/tmp")
        .spawn()
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let client = Client::new();

    // /compress 200 (pas de body nécessaire pour une 400 vs 401, on vérifie juste pas 401)
    let res = client
        .post(format!("http://127.0.0.1:{}/compress", port))
        .send()
        .await
        .unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED); // sera probablement 415/422/400 mais pas 401
    child1.kill().unwrap();
    child1.wait().unwrap();

    let Some(port2) = get_free_port() else { return };
    // Test server avec token
    let mut child2 = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .arg("serve")
        .arg("--bind")
        .arg(format!("127.0.0.1:{}", port2))
        .arg("--auth-token")
        .arg("secret-token")
        .env_clear()
        .env("HOME", "/tmp")
        .env("USERPROFILE", "/tmp")
        .env("LM_RESIZER_STATE_DIR", "/tmp")
        .spawn()
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    // sans en-tête -> 401
    let res = client
        .post(format!("http://127.0.0.1:{}/compress", port2))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // mauvais jeton -> 401
    let res = client
        .post(format!("http://127.0.0.1:{}/compress", port2))
        .header("Authorization", "Bearer bad-token")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // bon jeton -> pas 401
    let res = client
        .post(format!("http://127.0.0.1:{}/compress", port2))
        .header("Authorization", "Bearer secret-token")
        .send()
        .await
        .unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);

    // /health sans jeton -> 200
    let res = client
        .get(format!("http://127.0.0.1:{}/health", port2))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    child2.kill().unwrap();
    child2.wait().unwrap();
}

#[test]
fn auth_empty_token_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_lm-resizer"))
        .args(["serve", "--bind", "0.0.0.0:0", "--auth-token", "   "])
        .env_clear()
        .env("HOME", "/tmp")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("ne doit pas être vide"));
}
