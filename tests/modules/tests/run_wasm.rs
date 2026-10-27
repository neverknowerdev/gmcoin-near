//! The exact module binaries we ship, run under wasmtime with wasi-http
//! against a local wiremock "CachedXApi". Checks stdin/stdout JSON end to end.
//! Needs `wasmtime` on PATH and `scripts/build-modules.sh` run first; skips otherwise.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};
use wiremock::matchers::{header, method, path, query_param, query_param_contains};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../../crates/x-client/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn module(name: &str) -> Option<String> {
    let p = format!("{}/../../res/{name}.wasm", env!("CARGO_MANIFEST_DIR"));
    let have_wasmtime = Command::new("wasmtime").arg("--version").output().is_ok();
    if !std::path::Path::new(&p).exists() || !have_wasmtime {
        eprintln!("skipping: needs wasmtime and {p} (scripts/build-modules.sh)");
        return None;
    }
    Some(p)
}

fn run(wasm: &str, base: &str, input: &Value) -> Value {
    let mut child = Command::new("wasmtime")
        .args(["run", "-S", "http", "-S", "inherit-network"])
        .args([
            "--env",
            &format!("X_API_BASE={base}"),
            "--env",
            "X_API_KEY=test-key",
        ])
        .arg(wasm)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("wasmtime");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "wasmtime failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("bad stdout {e}: {}", String::from_utf8_lossy(&out.stdout)))
}

async fn x_server() -> MockServer {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/UserResultsByRestIds"))
        .and(header("x-api-key", "test-key"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("users.json")))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/Search"))
        .and(query_param("cursor", "CURSOR_PAGE_2"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("search_page2.json")))
        .with_priority(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/Search"))
        .and(query_param("cursor", "CURSOR_PAGE_3"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("search_empty.json")))
        .with_priority(1)
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/Search"))
        .and(query_param_contains("q", "since_time"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("search_page1.json")))
        .with_priority(5)
        .mount(&s)
        .await;
    s
}

#[tokio::test(flavor = "multi_thread")]
async fn x_scan_wasm_scores_a_day() {
    let Some(wasm) = module("x-scan") else { return };
    let s = x_server().await;
    let input = json!({
        "since": 1_793_491_200u64, "until": 1_793_577_600u64,
        "users": [[1, "101"], [2, "202"], [3, "303"]],
        "scoring": {
            "points_simple": 1, "points_hashtag": 3, "points_cashtag": 5, "points_per_like": 1,
            "max_hashtag_posts": 10, "max_cashtag_posts": 10, "max_simple_posts": 4294967295u32,
            "max_likes_per_post": 100, "spam_post_limit": 1000
        }
    });
    let uri = s.uri();
    let out = tokio::task::spawn_blocking(move || run(&wasm, &uri, &input))
        .await
        .unwrap();
    assert_eq!(out, json!({ "ok": true, "scores": [[1, 110], [2, 4]] }));
}

#[tokio::test(flavor = "multi_thread")]
async fn x_scan_wasm_reports_rate_limits() {
    let Some(wasm) = module("x-scan") else { return };
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&s)
        .await;
    let input = json!({ "since": 0, "until": 1, "users": [[1, "101"]] });
    let uri = s.uri();
    let out = tokio::task::spawn_blocking(move || run(&wasm, &uri, &input))
        .await
        .unwrap();
    assert_eq!(
        out,
        json!({ "ok": false, "scores": [], "error": "X API status 429" })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn x_verify_wasm_finds_the_code() {
    let Some(wasm) = module("x-verify") else {
        return;
    };
    let code = gm_scoring::link_code::generate("alice.testnet", [1, 2, 3]);
    let tweet = fixture("tweet.json").replace("CODE_PLACEHOLDER", &code);
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/TweetResultByRestId"))
        .and(query_param("tweet_id", "1850000000000000099"))
        .respond_with(ResponseTemplate::new(200).set_body_string(tweet))
        .mount(&s)
        .await;
    let input =
        json!({ "code": code, "account_id": "alice.testnet", "post_id": "1850000000000000099" });
    let uri = s.uri();
    let out = tokio::task::spawn_blocking(move || run(&wasm, &uri, &input))
        .await
        .unwrap();
    assert_eq!(
        out,
        json!({ "ok": true, "x_id": "101", "handle": "alice_gm", "post_id": "1850000000000000099" })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn x_verify_wasm_rejects_bad_input() {
    let Some(wasm) = module("x-verify") else {
        return;
    };
    let out = tokio::task::spawn_blocking(move || {
        run(&wasm, "http://127.0.0.1:9", &json!({ "nope": 1 }))
    })
    .await
    .unwrap();
    assert_eq!(out["ok"], false);
    assert!(out["error"].as_str().unwrap().starts_with("bad input"));
}
