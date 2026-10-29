//! Defensive parsers for CachedXApi responses (X GraphQL shapes, as v1 used).
//! Unknown or partial entries are skipped rather than failing the whole page.

use serde_json::Value;

use crate::Tweet;

fn s(v: &Value) -> Option<String> {
    match v {
        Value::String(x) if !x.is_empty() => Some(x.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Reads a tweet object in either `result` or `result.tweet` form.
pub fn tweet_from_result(result: &Value) -> Option<Tweet> {
    let r = if result.get("legacy").is_some() {
        result
    } else {
        result.get("tweet")?
    };
    let legacy = r.get("legacy")?;
    let user = r.pointer("/core/user_results/result")?;
    let author_id = s(user.get("rest_id")?).or_else(|| s(legacy.get("user_id_str")?))?;
    let author_handle = user
        .pointer("/core/screen_name")
        .or_else(|| user.pointer("/legacy/screen_name"))
        .and_then(s)
        .unwrap_or_default();
    Some(Tweet {
        id: s(r.get("rest_id")?).or_else(|| s(legacy.get("id_str")?))?,
        author_id,
        author_handle,
        text: s(legacy.get("full_text")?)?,
        likes: legacy
            .get("favorite_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(u32::MAX as u64) as u32,
        created_at: legacy
            .get("created_at")
            .and_then(Value::as_str)
            .and_then(parse_created_at),
    })
}

/// `data.search_by_raw_query.search_timeline.timeline.instructions[]`:
/// tweets from `entries[].content.content.tweet_results.result` (or
/// `content.itemContent.tweet_results.result`), the cursor from the entry
/// whose `cursor_type` is `Bottom`.
pub fn search_response(v: &Value) -> (Vec<Tweet>, Option<String>) {
    let mut tweets = Vec::new();
    let mut cursor = None;
    let Some(instructions) = v
        .pointer("/data/search_by_raw_query/search_timeline/timeline/instructions")
        .and_then(Value::as_array)
    else {
        return (tweets, cursor);
    };
    let mut take_cursor = |c: &Value| {
        if c.get("cursor_type").and_then(Value::as_str) == Some("Bottom") {
            cursor = c.get("value").and_then(s);
        }
    };
    for ins in instructions {
        if let Some(c) = ins.pointer("/entry/content") {
            take_cursor(c);
        }
        for e in ins
            .get("entries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(content) = e.get("content") else {
                continue;
            };
            take_cursor(content);
            let result = content
                .pointer("/content/tweet_results/result")
                .or_else(|| content.pointer("/itemContent/tweet_results/result"));
            if let Some(t) = result.and_then(tweet_from_result) {
                tweets.push(t);
            }
        }
    }
    (tweets, cursor)
}

/// One tweet: `data.tweet_results.result` (GraphQL), `data.tweetResult.result`,
/// or a flat `{ id, text, author_id }`.
pub fn tweet_response(v: &Value, id: &str) -> Option<Tweet> {
    for p in [
        "/data/tweet_results/result",
        "/data/tweetResult/result",
        "/tweet_results/result",
        "/result",
    ] {
        if let Some(t) = v.pointer(p).and_then(tweet_from_result) {
            return Some(t);
        }
    }
    let text = v.get("text").and_then(s)?;
    let author_id = v.get("author_id").and_then(s)?;
    Some(Tweet {
        id: v.get("id").and_then(s).unwrap_or_else(|| id.to_string()),
        author_id,
        author_handle: v.get("username").and_then(s).unwrap_or_default(),
        text,
        likes: 0,
        created_at: None,
    })
}

/// `data.users[]` of `{ rest_id, result: { core: { screen_name } } }` (v1 shape),
/// or `{ result: { rest_id, core | legacy: { screen_name } } }`.
pub fn users_response(v: &Value) -> Vec<(String, String)> {
    let Some(users) = v.pointer("/data/users").and_then(Value::as_array) else {
        return vec![];
    };
    users
        .iter()
        .filter_map(|u| {
            let r = u.get("result").unwrap_or(u);
            let id = u
                .get("rest_id")
                .and_then(s)
                .or_else(|| r.get("rest_id").and_then(s))?;
            let handle = r
                .pointer("/core/screen_name")
                .or_else(|| r.pointer("/legacy/screen_name"))
                .and_then(s)?;
            Some((id, handle))
        })
        .collect()
}

/// Parses X's `created_at`, e.g. `Wed Oct 10 20:19:24 +0000 2018`, to unix seconds (UTC only).
pub fn parse_created_at(t: &str) -> Option<u64> {
    let p: Vec<&str> = t.split_whitespace().collect();
    if p.len() != 6 || p[4] != "+0000" {
        return None;
    }
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == p[1])? as u64
        + 1;
    let day: u64 = p[2].parse().ok()?;
    let year: i64 = p[5].parse().ok()?;
    let hms: Vec<u64> = p[3]
        .split(':')
        .map(|x| x.parse().ok())
        .collect::<Option<_>>()?;
    if hms.len() != 3 {
        return None;
    }
    let days = days_from_civil(year, month as i64, day as i64);
    if days < 0 {
        return None;
    }
    Some(days as u64 * 86_400 + hms[0] * 3_600 + hms[1] * 60 + hms[2])
}

// Howard Hinnant's algorithm: days since 1970-01-01.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_at_parses() {
        assert_eq!(parse_created_at("Thu Jan 01 00:00:00 +0000 1970"), Some(0));
        assert_eq!(
            parse_created_at("Wed Oct 10 20:19:24 +0000 2018"),
            Some(1_539_202_764)
        );
        assert_eq!(
            parse_created_at("Sun Nov 01 12:00:00 +0000 2026"),
            Some(1_793_534_400)
        );
        assert_eq!(parse_created_at("garbage"), None);
        assert_eq!(parse_created_at("Wed Oct 10 20:19:24 +0200 2018"), None);
    }
}
