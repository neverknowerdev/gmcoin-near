//! GM scoring rules (from v1's twitter-worker) and X link codes.
//!
//! A post scores in one category: `$gm` (5), `#gm` (3) or a plain `gm` word (1),
//! plus 1 point per like. `$gm` and `#gm` posts are capped per user per day;
//! a capped post counts for nothing, likes included. A user with an absurd
//! number of GM posts in a day is treated as a bot and gets nothing.

#![cfg_attr(not(test), no_std)]
extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

pub mod link_code;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Keyword {
    Simple,
    Hashtag,
    Cashtag,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ScoringConfig {
    pub points_simple: u32,
    pub points_hashtag: u32,
    pub points_cashtag: u32,
    pub points_per_like: u32,
    /// Max `#gm` posts counted per user per day.
    pub max_hashtag_posts: u32,
    /// Max `$gm` posts counted per user per day.
    pub max_cashtag_posts: u32,
    /// Max plain `gm` posts counted per user per day (v1 had no cap).
    pub max_simple_posts: u32,
    /// Likes counted per post. v1 re-checked posts above 100 likes with the
    /// official API; until that check exists we count at most this many.
    pub max_likes_per_post: u32,
    /// Users with at least this many GM posts in a day score zero.
    pub spam_post_limit: u32,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            points_simple: 1,
            points_hashtag: 3,
            points_cashtag: 5,
            points_per_like: 1,
            max_hashtag_posts: 10,
            max_cashtag_posts: 10,
            max_simple_posts: u32::MAX,
            max_likes_per_post: 100,
            spam_post_limit: 1_000,
        }
    }
}

/// Finds the GM keyword of a post, as v1 did: split on whitespace, strip
/// `.,!?;:()`, compare case-insensitively. `$gm` beats `#gm` beats `gm`.
pub fn find_keyword(text: &str) -> Option<Keyword> {
    let mut found = None;
    for word in text.split_whitespace() {
        let cleaned: String = word
            .chars()
            .filter(|c| !matches!(c, '.' | ',' | '!' | '?' | ';' | ':' | '(' | ')'))
            .flat_map(|c| c.to_lowercase())
            .collect();
        match cleaned.as_str() {
            "$gm" => return Some(Keyword::Cashtag),
            "#gm" => found = Some(Keyword::Hashtag),
            "gm" if found.is_none() => found = Some(Keyword::Simple),
            _ => {}
        }
    }
    found
}

/// One post as the scan module sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post<'a> {
    pub author_id: &'a str,
    pub post_id: &'a str,
    pub text: &'a str,
    pub likes: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DayTally {
    pub simple: u32,
    pub hashtag: u32,
    pub cashtag: u32,
    pub likes: u64,
    /// GM posts seen, including capped ones.
    pub posts: u32,
}

impl DayTally {
    pub fn points(&self, cfg: &ScoringConfig) -> u64 {
        if self.posts >= cfg.spam_post_limit {
            return 0;
        }
        self.simple as u64 * cfg.points_simple as u64
            + self.hashtag as u64 * cfg.points_hashtag as u64
            + self.cashtag as u64 * cfg.points_cashtag as u64
            + self.likes * cfg.points_per_like as u64
    }
}

/// Tallies one user's day. Duplicate post ids are counted once.
#[derive(Clone, Debug, Default)]
pub struct Scorer {
    tallies: BTreeMap<String, DayTally>,
    seen: BTreeMap<String, ()>,
}

impl Scorer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a post; returns the keyword it counted under, if it counted.
    pub fn add(&mut self, cfg: &ScoringConfig, post: &Post) -> Option<Keyword> {
        if self.seen.insert(String::from(post.post_id), ()).is_some() {
            return None;
        }
        let kw = find_keyword(post.text)?;
        let t = self
            .tallies
            .entry(String::from(post.author_id))
            .or_default();
        t.posts += 1;
        let counted = match kw {
            Keyword::Cashtag if t.cashtag < cfg.max_cashtag_posts => {
                t.cashtag += 1;
                true
            }
            Keyword::Hashtag if t.hashtag < cfg.max_hashtag_posts => {
                t.hashtag += 1;
                true
            }
            Keyword::Simple if t.simple < cfg.max_simple_posts => {
                t.simple += 1;
                true
            }
            _ => false,
        };
        if !counted {
            return None;
        }
        t.likes += post.likes.min(cfg.max_likes_per_post) as u64;
        Some(kw)
    }

    pub fn tally(&self, author_id: &str) -> Option<&DayTally> {
        self.tallies.get(author_id)
    }

    /// Points per author, zero scores left out, sorted by author id.
    pub fn points(&self, cfg: &ScoringConfig) -> Vec<(String, u64)> {
        self.tallies
            .iter()
            .map(|(id, t)| (id.clone(), t.points(cfg)))
            .filter(|(_, p)| *p > 0)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn post<'a>(author: &'a str, id: &'a str, text: &'a str, likes: u32) -> Post<'a> {
        Post {
            author_id: author,
            post_id: id,
            text,
            likes,
        }
    }

    #[test]
    fn keywords_match_v1() {
        assert_eq!(find_keyword("gm"), Some(Keyword::Simple));
        assert_eq!(find_keyword("GM frens!"), Some(Keyword::Simple));
        assert_eq!(find_keyword("gm!!"), Some(Keyword::Simple));
        assert_eq!(find_keyword("(gm)"), Some(Keyword::Simple));
        assert_eq!(find_keyword("hello #GM,"), Some(Keyword::Hashtag));
        assert_eq!(find_keyword("gm #gm"), Some(Keyword::Hashtag));
        assert_eq!(find_keyword("#gm gm $GM"), Some(Keyword::Cashtag));
        assert_eq!(find_keyword("gmgm"), None);
        assert_eq!(find_keyword("@gm"), None);
        assert_eq!(find_keyword("gm-gm"), None);
        assert_eq!(find_keyword("good morning"), None);
        assert_eq!(find_keyword(""), None);
    }

    #[test]
    fn points_follow_v1_weights() {
        let cfg = ScoringConfig::default();
        let mut s = Scorer::new();
        s.add(&cfg, &post("1", "a", "gm", 2));
        s.add(&cfg, &post("1", "b", "#gm", 0));
        s.add(&cfg, &post("1", "c", "$gm", 1));
        s.add(&cfg, &post("2", "d", "nothing here", 50));
        assert_eq!(s.points(&cfg), vec![("1".into(), 1 + 2 + 3 + 5 + 1)]);
    }

    #[test]
    fn hashtag_and_cashtag_are_capped_and_capped_posts_lose_likes() {
        let cfg = ScoringConfig::default();
        let mut s = Scorer::new();
        let ids: Vec<String> = (0..12).map(|i| format!("p{i}")).collect();
        for id in &ids {
            s.add(&cfg, &post("1", id, "#gm", 1));
        }
        let t = s.tally("1").unwrap();
        assert_eq!((t.hashtag, t.likes, t.posts), (10, 10, 12));
        assert_eq!(t.points(&cfg), 10 * 3 + 10);
    }

    #[test]
    fn likes_are_capped_per_post() {
        let cfg = ScoringConfig::default();
        let mut s = Scorer::new();
        s.add(&cfg, &post("1", "a", "gm", 10_000));
        assert_eq!(s.points(&cfg)[0].1, 1 + 100);
    }

    #[test]
    fn duplicate_posts_count_once() {
        let cfg = ScoringConfig::default();
        let mut s = Scorer::new();
        assert!(s.add(&cfg, &post("1", "a", "gm", 1)).is_some());
        assert!(s.add(&cfg, &post("1", "a", "gm", 1)).is_none());
        assert_eq!(s.points(&cfg)[0].1, 2);
    }

    #[test]
    fn spammers_score_zero() {
        let cfg = ScoringConfig {
            spam_post_limit: 3,
            ..Default::default()
        };
        let mut s = Scorer::new();
        for id in ["a", "b", "c"] {
            s.add(&cfg, &post("1", id, "gm", 0));
        }
        assert!(s.points(&cfg).is_empty());
    }

    proptest! {
        #[test]
        fn daily_points_are_bounded(posts in proptest::collection::vec((0u8..4, 0u32..1_000_000), 0..300)) {
            let cfg = ScoringConfig::default();
            let mut s = Scorer::new();
            let texts = ["gm", "#gm", "$gm", "hello"];
            let ids: Vec<String> = (0..posts.len()).map(|i| format!("{i}")).collect();
            for (i, (k, likes)) in posts.iter().enumerate() {
                s.add(&cfg, &post("u", &ids[i], texts[*k as usize], *likes));
            }
            if let Some(t) = s.tally("u") {
                prop_assert!(t.hashtag <= 10 && t.cashtag <= 10);
                let counted = (t.simple + t.hashtag + t.cashtag) as u64;
                prop_assert!(t.likes <= counted * 100);
            }
        }

        #[test]
        fn find_keyword_never_panics(text in ".*") {
            let _ = find_keyword(&text);
        }
    }
}
