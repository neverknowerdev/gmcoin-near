//! CachedXApi client and the logic of the two OutLayer jobs:
//! - `verify`: find a link code by X search (or read a pasted post) and return its author,
//! - `scan`: fetch one batch of users' GM posts for a day and score them.
//!
//! All HTTP goes through [`HttpClient`], so the logic is tested natively with
//! fixtures, and the WASM modules plug in a wasi-http implementation.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod parse;

use gm_scoring::{link_code, Post, Scorer, ScoringConfig};

pub trait HttpClient {
    /// GET `url` with `headers`; returns the status code and body.
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<(u16, Vec<u8>), String>;
}

/// Where and how to reach CachedXApi. Read from OutLayer secrets (env vars),
/// so testnet points at the mock X server and mainnet at the real provider
/// with identical code.
#[derive(Clone, Debug)]
pub struct XApiConfig {
    pub base_url: String,
    pub api_key: String,
    pub key_header: String,
    pub search_path: String,
    pub users_path: String,
    pub tweet_path: String,
    pub max_pages: u32,
}

impl XApiConfig {
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let base_url = get("X_API_BASE").ok_or("X_API_BASE not set")?;
        let api_key = get("X_API_KEY").ok_or("X_API_KEY not set")?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            key_header: get("X_API_KEY_HEADER").unwrap_or_else(|| "x-api-key".into()),
            search_path: get("X_API_SEARCH_PATH").unwrap_or_else(|| "/Search".into()),
            users_path: get("X_API_USERS_PATH").unwrap_or_else(|| "/UserResultsByRestIds".into()),
            tweet_path: get("X_API_TWEET_PATH").unwrap_or_else(|| "/TweetResultByRestId".into()),
            max_pages: get("X_API_MAX_PAGES")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        })
    }
}

pub struct CachedXApi<'a, H: HttpClient> {
    pub http: &'a H,
    pub cfg: &'a XApiConfig,
}

/// A post as returned by the provider, flattened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tweet {
    pub id: String,
    pub author_id: String,
    pub author_handle: String,
    pub text: String,
    pub likes: u32,
    /// Unix seconds, when the provider gives `created_at`.
    pub created_at: Option<u64>,
}

impl<H: HttpClient> CachedXApi<'_, H> {
    fn get_json(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, String> {
        let mut url = format!("{}{}", self.cfg.base_url, path);
        for (i, (k, v)) in query.iter().enumerate() {
            url.push(if i == 0 { '?' } else { '&' });
            url.push_str(k);
            url.push('=');
            url.push_str(&encode(v));
        }
        let headers = [(self.cfg.key_header.clone(), self.cfg.api_key.clone())];
        let (status, body) = self.http.get(&url, &headers)?;
        if !(200..300).contains(&status) {
            return Err(format!("X API status {status}"));
        }
        serde_json::from_slice(&body).map_err(|e| format!("X API bad JSON: {e}"))
    }

    /// One page of "Latest" search results and the next cursor.
    pub fn search_page(
        &self,
        q: &str,
        cursor: Option<&str>,
    ) -> Result<(Vec<Tweet>, Option<String>), String> {
        let mut query = vec![
            ("q", q),
            ("type", "Latest"),
            ("count", "20"),
            ("safe_search", "false"),
        ];
        if let Some(c) = cursor {
            query.push(("cursor", c));
        }
        let v = self.get_json(&self.cfg.search_path, &query)?;
        Ok(parse::search_response(&v))
    }

    /// All pages of a search, up to `max_pages`.
    pub fn search(&self, q: &str) -> Result<Vec<Tweet>, String> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        for _ in 0..self.cfg.max_pages {
            let (page, next) = self.search_page(q, cursor.as_deref())?;
            let empty = page.is_empty();
            out.extend(page);
            match next {
                Some(c) if !empty && Some(&c) != cursor.as_ref() => cursor = Some(c),
                _ => break,
            }
        }
        Ok(out)
    }

    pub fn tweet(&self, id: &str) -> Result<Option<Tweet>, String> {
        let v = self.get_json(&self.cfg.tweet_path, &[("tweet_id", id)])?;
        Ok(parse::tweet_response(&v, id))
    }

    /// X ids -> current handles (100 per request).
    pub fn handles(&self, ids: &[String]) -> Result<Vec<(String, String)>, String> {
        let mut out = Vec::new();
        for chunk in ids.chunks(100) {
            let v = self.get_json(&self.cfg.users_path, &[("user_ids", &chunk.join(","))])?;
            out.extend(parse::users_response(&v));
        }
        Ok(out)
    }
}

// ---------------- x-verify ----------------

#[derive(Clone, Debug, Deserialize)]
pub struct VerifyInput {
    pub code: String,
    pub account_id: String,
    pub post_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct VerifyOutput {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl VerifyOutput {
    fn fail(e: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(e.into()),
            ..Default::default()
        }
    }
}

/// Finds the post carrying the link code. With a pasted post id, reads that
/// post; otherwise searches for the code. If several accounts posted the code,
/// the earliest post wins (the real owner posts first; copies come later).
pub fn verify<H: HttpClient>(api: &CachedXApi<H>, input: &VerifyInput) -> VerifyOutput {
    if !link_code::validate(&input.code, &input.account_id) {
        return VerifyOutput::fail("code does not belong to the account");
    }
    let found = match &input.post_id {
        Some(id) => match api.tweet(id) {
            Ok(Some(t)) if link_code::post_contains(&t.text, &input.code) => Some(t),
            Ok(Some(_)) => return VerifyOutput::fail("post does not contain the code"),
            Ok(None) => return VerifyOutput::fail("post not found"),
            Err(e) => return VerifyOutput::fail(e),
        },
        None => match api.search(&format!("\"{}\"", input.code)) {
            Ok(tweets) => tweets
                .into_iter()
                .filter(|t| link_code::post_contains(&t.text, &input.code))
                .min_by_key(|t| (t.id.len(), t.id.clone())),
            Err(e) => return VerifyOutput::fail(e),
        },
    };
    match found {
        Some(t) => VerifyOutput {
            ok: true,
            x_id: Some(t.author_id),
            handle: Some(t.author_handle),
            post_id: Some(t.id),
            error: None,
        },
        None => VerifyOutput::fail("link code not found on X yet"),
    }
}

// ---------------- x-scan ----------------

#[derive(Clone, Debug, Deserialize)]
pub struct ScanInput {
    /// Day window, unix seconds, `[since, until)`.
    pub since: u64,
    pub until: u64,
    /// `[user id, X id]` pairs.
    pub users: Vec<(u64, String)>,
    #[serde(default)]
    pub scoring: Option<ScoringConfig>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct ScanOutput {
    pub ok: bool,
    /// `[user id, points]`, users with zero points left out.
    pub scores: Vec<(u64, u64)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub const MAX_QUERY_LEN: usize = 512;

/// Builds search queries `(gm OR #gm OR $gm) since_time:S until_time:U (from:a OR from:b ...)`,
/// each under `MAX_QUERY_LEN`, as v1 did.
pub fn build_queries(since: u64, until: u64, handles: &[String]) -> Vec<String> {
    let prefix = format!("(gm OR #gm OR $gm) since_time:{since} until_time:{until} (");
    let mut out = Vec::new();
    let mut q = prefix.clone();
    let mut n = 0;
    for h in handles {
        let part = format!("{}from:{h}", if n > 0 { " OR " } else { "" });
        if n > 0 && q.len() + part.len() + 1 > MAX_QUERY_LEN {
            q.push(')');
            out.push(q);
            q = prefix.clone();
            n = 0;
            q.push_str(&format!("from:{h}"));
        } else {
            q.push_str(&part);
        }
        n += 1;
    }
    if n > 0 {
        q.push(')');
        out.push(q);
    }
    out
}

pub fn scan<H: HttpClient>(api: &CachedXApi<H>, input: &ScanInput) -> ScanOutput {
    let fail = |e: String| ScanOutput {
        ok: false,
        scores: vec![],
        error: Some(e),
    };
    let cfg = input.scoring.clone().unwrap_or_default();
    if input.users.is_empty() {
        return ScanOutput {
            ok: true,
            scores: vec![],
            error: None,
        };
    }
    let ids: Vec<String> = input.users.iter().map(|(_, x)| x.clone()).collect();
    let handles = match api.handles(&ids) {
        Ok(h) => h,
        Err(e) => return fail(e),
    };
    let names: Vec<String> = handles.iter().map(|(_, h)| h.clone()).collect();
    let mut scorer = Scorer::new();
    for q in build_queries(input.since, input.until, &names) {
        let tweets = match api.search(&q) {
            Ok(t) => t,
            Err(e) => return fail(e),
        };
        for t in tweets {
            if !ids.contains(&t.author_id) {
                continue; // not one of this batch's users
            }
            if let Some(ts) = t.created_at {
                if ts < input.since || ts >= input.until {
                    continue;
                }
            }
            scorer.add(
                &cfg,
                &Post {
                    author_id: &t.author_id,
                    post_id: &t.id,
                    text: &t.text,
                    likes: t.likes,
                },
            );
        }
    }
    let by_x = scorer.points(&cfg);
    let scores = input
        .users
        .iter()
        .filter_map(|(uid, x)| by_x.iter().find(|(id, _)| id == x).map(|(_, p)| (*uid, *p)))
        .collect();
    ScanOutput {
        ok: true,
        scores,
        error: None,
    }
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_split_under_512_chars() {
        let handles: Vec<String> = (0..60)
            .map(|i| format!("user_with_long_name_{i:02}"))
            .collect();
        let qs = build_queries(1_700_000_000, 1_700_086_400, &handles);
        assert!(qs.len() > 1);
        for q in &qs {
            assert!(q.len() <= MAX_QUERY_LEN, "{} > 512", q.len());
            assert!(q.starts_with(
                "(gm OR #gm OR $gm) since_time:1700000000 until_time:1700086400 (from:"
            ));
            assert!(q.ends_with(')'));
            assert!(!q.contains("( OR"));
        }
        let total: usize = qs.iter().map(|q| q.matches("from:").count()).sum();
        assert_eq!(total, 60);
    }

    #[test]
    fn encodes_queries() {
        assert_eq!(encode("gm OR #gm"), "gm%20OR%20%23gm");
        assert_eq!(encode("\"GM07\""), "%22GM07%22");
    }
}
