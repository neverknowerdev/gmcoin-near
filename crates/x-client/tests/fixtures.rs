//! x-verify and x-scan logic against fixture responses in the CachedXApi
//! (X GraphQL) shape that v1's twitterRequester.ts parsed.

use std::cell::RefCell;

use x_client::*;

const DAY_START: u64 = 1_793_491_200; // 2026-11-01 00:00 UTC
const DAY_END: u64 = DAY_START + 86_400;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

struct FakeHttp {
    routes: Vec<(String, u16, String)>,
    calls: RefCell<Vec<String>>,
}

impl FakeHttp {
    fn new(routes: Vec<(&str, u16, String)>) -> Self {
        Self {
            routes: routes
                .into_iter()
                .map(|(p, s, b)| (p.to_string(), s, b))
                .collect(),
            calls: RefCell::new(vec![]),
        }
    }
}

impl HttpClient for FakeHttp {
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<(u16, Vec<u8>), String> {
        assert_eq!(headers, &[("x-api-key".to_string(), "k".to_string())]);
        self.calls.borrow_mut().push(url.to_string());
        // The most specific (longest) matching route wins.
        self.routes
            .iter()
            .filter(|(p, _, _)| url.contains(p.as_str()))
            .max_by_key(|(p, _, _)| p.len())
            .map(|(_, s, b)| (*s, b.clone().into_bytes()))
            .ok_or_else(|| format!("no route for {url}"))
    }
}

fn cfg() -> XApiConfig {
    XApiConfig::from_env(|k| match k {
        "X_API_BASE" => Some("https://mock-x.example/".into()),
        "X_API_KEY" => Some("k".into()),
        _ => None,
    })
    .unwrap()
}

fn scan_routes() -> Vec<(&'static str, u16, String)> {
    vec![
        ("/UserResultsByRestIds", 200, fixture("users.json")),
        ("/Search?q=", 200, fixture("search_page1.json")),
        ("cursor=CURSOR_PAGE_2", 200, fixture("search_page2.json")),
        ("cursor=CURSOR_PAGE_3", 200, fixture("search_empty.json")),
    ]
}

fn scan_input() -> ScanInput {
    serde_json::from_value(serde_json::json!({
        "since": DAY_START,
        "until": DAY_END,
        "users": [[1, "101"], [2, "202"], [3, "303"]],
    }))
    .unwrap()
}

#[test]
fn scan_scores_a_day_like_v1() {
    let http = FakeHttp::new(scan_routes());
    let c = cfg();
    let api = CachedXApi {
        http: &http,
        cfg: &c,
    };
    let out = scan(&api, &scan_input());
    // alice: gm (1) + 4 likes + $gm (5) + 250 likes capped at 100; duplicate post ignored.
    // bob: #gm (3) + 1 like; yesterday's post is outside the window.
    // The stranger is not in the batch; carol posted nothing.
    assert_eq!(
        out,
        ScanOutput {
            ok: true,
            scores: vec![(1, 110), (2, 4)],
            error: None
        }
    );

    let calls = http.calls.borrow();
    assert!(calls[0]
        .starts_with("https://mock-x.example/UserResultsByRestIds?user_ids=101%2C202%2C303"));
    assert!(calls[1].contains("since_time%3A1793491200"));
    assert!(calls[1].contains("from%3Aalice_gm%20OR%20from%3Abob%20OR%20from%3Acarol"));
    assert_eq!(
        calls.len(),
        4,
        "users + 3 search pages, stops at the empty page"
    );
}

#[test]
fn scan_output_fits_on_chain_limits() {
    let http = FakeHttp::new(scan_routes());
    let c = cfg();
    let api = CachedXApi {
        http: &http,
        cfg: &c,
    };
    let out = scan(&api, &scan_input());
    // 25 users of 20-digit ids with 6-digit points stay well under OutLayer's on-chain output size.
    let big = ScanOutput {
        ok: true,
        scores: (0..25).map(|i| (1_000_000_000 + i, 999_999)).collect(),
        error: None,
    };
    assert!(serde_json::to_string(&big).unwrap().len() < 900);
    assert!(serde_json::to_string(&out).unwrap().len() < 100);
}

#[test]
fn scan_reports_provider_errors() {
    let mut routes = scan_routes();
    routes[1] = ("/Search?q=", 429, "{\"error\":\"rate limited\"}".into());
    let http = FakeHttp::new(routes);
    let c = cfg();
    let out = scan(
        &CachedXApi {
            http: &http,
            cfg: &c,
        },
        &scan_input(),
    );
    assert!(!out.ok);
    assert_eq!(out.error.as_deref(), Some("X API status 429"));
}

#[test]
fn scan_rejects_bad_json_as_error() {
    let mut routes = scan_routes();
    routes[0] = ("/UserResultsByRestIds", 200, "<html>".into());
    let http = FakeHttp::new(routes);
    let c = cfg();
    let out = scan(
        &CachedXApi {
            http: &http,
            cfg: &c,
        },
        &scan_input(),
    );
    assert!(!out.ok);
    assert!(out.error.unwrap().starts_with("X API bad JSON"));
}

fn code() -> String {
    gm_scoring::link_code::generate("alice.testnet", [5, 6, 7])
}

fn search_with_code(entries: &[(&str, &str, &str)]) -> String {
    // (post id, author id, handle)
    let items: Vec<serde_json::Value> = entries
        .iter()
        .map(|(id, author, handle)| {
            serde_json::json!({ "content": { "content": { "tweet_results": { "result": {
                "rest_id": id,
                "core": { "user_results": { "result": { "rest_id": author, "core": { "screen_name": handle } } } },
                "legacy": { "full_text": format!("Linking GM: {} gm", code()), "favorite_count": 0 }
            }}}}})
        })
        .collect();
    serde_json::json!({ "data": { "search_by_raw_query": { "search_timeline": { "timeline": {
        "instructions": [ { "entries": items } ] } } } } })
    .to_string()
}

#[test]
fn verify_by_search_takes_the_earliest_post() {
    let http = FakeHttp::new(vec![(
        "/Search?q=",
        200,
        search_with_code(&[
            ("1850000000000000500", "666", "copycat"),
            ("1850000000000000100", "101", "alice_gm"),
        ]),
    )]);
    let c = cfg();
    let input = VerifyInput {
        code: code(),
        account_id: "alice.testnet".into(),
        post_id: None,
    };
    let out = verify(
        &CachedXApi {
            http: &http,
            cfg: &c,
        },
        &input,
    );
    assert_eq!(out.x_id.as_deref(), Some("101"));
    assert_eq!(out.handle.as_deref(), Some("alice_gm"));
    assert_eq!(out.post_id.as_deref(), Some("1850000000000000100"));
    assert!(out.ok);
    assert!(http.calls.borrow()[0].contains(&format!("q=%22{}%22", code())));
}

#[test]
fn verify_by_pasted_post() {
    let tweet = fixture("tweet.json").replace("CODE_PLACEHOLDER", &code().to_lowercase());
    let http = FakeHttp::new(vec![(
        "/TweetResultByRestId?tweet_id=1850000000000000099",
        200,
        tweet,
    )]);
    let c = cfg();
    let input = VerifyInput {
        code: code(),
        account_id: "alice.testnet".into(),
        post_id: Some("1850000000000000099".into()),
    };
    let out = verify(
        &CachedXApi {
            http: &http,
            cfg: &c,
        },
        &input,
    );
    assert!(out.ok, "{out:?}");
    assert_eq!(out.x_id.as_deref(), Some("101"));
}

#[test]
fn verify_failures() {
    let c = cfg();
    // Code for another account.
    let http = FakeHttp::new(vec![]);
    let bad = VerifyInput {
        code: code(),
        account_id: "bob.testnet".into(),
        post_id: None,
    };
    assert_eq!(
        verify(
            &CachedXApi {
                http: &http,
                cfg: &c
            },
            &bad
        )
        .error
        .as_deref(),
        Some("code does not belong to the account")
    );
    // Not posted yet.
    let http = FakeHttp::new(vec![("/Search?q=", 200, fixture("search_empty.json"))]);
    let input = VerifyInput {
        code: code(),
        account_id: "alice.testnet".into(),
        post_id: None,
    };
    let out = verify(
        &CachedXApi {
            http: &http,
            cfg: &c,
        },
        &input,
    );
    assert!(!out.ok);
    assert_eq!(out.error.as_deref(), Some("link code not found on X yet"));
    // Pasted post without the code.
    let http = FakeHttp::new(vec![("/TweetResultByRestId", 200, fixture("tweet.json"))]);
    let input = VerifyInput {
        post_id: Some("1".into()),
        ..input
    };
    let out = verify(
        &CachedXApi {
            http: &http,
            cfg: &c,
        },
        &input,
    );
    assert_eq!(out.error.as_deref(), Some("post does not contain the code"));
}

#[test]
fn output_json_shape_matches_core() {
    let out = VerifyOutput {
        ok: true,
        x_id: Some("1".into()),
        handle: Some("a".into()),
        post_id: None,
        error: None,
    };
    assert_eq!(
        serde_json::to_string(&out).unwrap(),
        r#"{"ok":true,"x_id":"1","handle":"a"}"#
    );
    let out = ScanOutput {
        ok: true,
        scores: vec![(1, 2)],
        error: None,
    };
    assert_eq!(
        serde_json::to_string(&out).unwrap(),
        r#"{"ok":true,"scores":[[1,2]]}"#
    );
}
