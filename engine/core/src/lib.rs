//! AdNox core filtering engine.
//!
//! This crate is the cross-platform heart of AdNox. It is consumed by:
//!   * `adnox-dns`  – the local DNS filtering server (Windows service / Android VPN resolver)
//!   * `adnox-ffi`  – the C ABI / JNI bridge used by the Android app and desktop UI
//!   * `adnox-cli`  – the developer / power-user command line tool
//!
//! Two levels of blocking are supported:
//!   1. **DNS / domain level** – decide whether a hostname should be resolved at all.
//!      Fast, works for every app on the device, no certificate required.
//!   2. **URL / network-request level** – full Adblock Plus rule matching (used once the
//!      proxy / HTTPS-inspection layer exists). Backed by Brave's `adblock` engine.

use std::collections::HashSet;

use adblock::lists::{FilterSet, ParseOptions};
use adblock::request::Request;
use adblock::Engine;

/// A set of blocked domains that matches a hostname *and all of its subdomains*.
///
/// e.g. if `doubleclick.net` is in the set, then `ads.g.doubleclick.net` is blocked too.
#[derive(Default)]
pub struct DomainSet {
    domains: HashSet<String>,
}

impl DomainSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, domain: &str) {
        let d = normalize_domain(domain);
        if !d.is_empty() {
            self.domains.insert(d);
        }
    }

    pub fn len(&self) -> usize {
        self.domains.len()
    }

    pub fn is_empty(&self) -> bool {
        self.domains.is_empty()
    }

    /// Returns true if `host` or any of its parent domains is blocked.
    pub fn contains(&self, host: &str) -> bool {
        let host = normalize_domain(host);
        if host.is_empty() {
            return false;
        }
        // Walk parent domains: a.b.c.com -> b.c.com -> c.com
        let mut rest: &str = &host;
        loop {
            if self.domains.contains(rest) {
                return true;
            }
            match rest.find('.') {
                Some(idx) => rest = &rest[idx + 1..],
                None => return false,
            }
        }
    }
}

/// Normalise a hostname for comparison: lowercase, strip trailing dot / port / whitespace.
fn normalize_domain(d: &str) -> String {
    let d = d.trim().trim_end_matches('.').to_ascii_lowercase();
    // strip an optional port
    match d.split_once(':') {
        Some((host, _port)) => host.to_string(),
        None => d,
    }
}

/// The compiled blocklist: a URL-level engine plus a DNS-level domain set.
pub struct Blocklist {
    engine: Engine,
    domains: DomainSet,
    rule_count: usize,
}

impl Blocklist {
    /// Build a blocklist from the raw text of one or more filter lists.
    ///
    /// Supports, line by line:
    ///   * Adblock Plus syntax (`||ads.example.com^`, `/banner/*`, …)  – fed to the URL engine
    ///   * hosts syntax (`0.0.0.0 ads.example.com`)                    – added to the domain set
    ///   * plain domains (`ads.example.com`)                           – added to the domain set
    ///   * comments (`!`, `#`) and blank lines                         – ignored
    pub fn from_lists(lists: &[String]) -> Self {
        let mut abp_rules: Vec<String> = Vec::new();
        let mut domains = DomainSet::new();
        let mut rule_count = 0usize;

        for list in lists {
            for raw in list.lines() {
                let line = raw.trim();
                if line.is_empty() || line.starts_with('!') {
                    continue;
                }
                // Comment in hosts files
                if line.starts_with('#') {
                    continue;
                }
                rule_count += 1;

                if let Some(domain) = parse_hosts_line(line) {
                    domains.insert(domain);
                    continue;
                }
                if let Some(domain) = parse_simple_domain_anchor(line) {
                    domains.insert(domain);
                }
                // Always feed the raw rule to the URL engine too; it understands ABP syntax
                // and will ignore things it cannot parse.
                abp_rules.push(line.to_string());
            }
        }

        let mut fs = FilterSet::new(false);
        fs.add_filters(&abp_rules, ParseOptions::default());
        let engine = Engine::from_filter_set(fs, true);

        Blocklist {
            engine,
            domains,
            rule_count,
        }
    }

    pub fn rule_count(&self) -> usize {
        self.rule_count
    }

    pub fn domain_count(&self) -> usize {
        self.domains.len()
    }

    /// DNS-level decision: should this hostname be blocked (sinkholed)?
    pub fn is_blocked_domain(&self, host: &str) -> bool {
        self.domains.contains(host)
    }

    /// URL-level decision: should this full network request be blocked?
    ///
    /// `request_type` is one of the Adblock resource types, e.g. "script", "image",
    /// "stylesheet", "xmlhttprequest", "sub_frame", "document", "other".
    pub fn is_blocked_request(&self, url: &str, source_url: &str, request_type: &str) -> bool {
        match Request::new(url, source_url, request_type) {
            Ok(req) => self.engine.check_network_request(&req).matched,
            Err(_) => false,
        }
    }
}

/// Parse a hosts-file line like `0.0.0.0 ads.example.com` -> `ads.example.com`.
fn parse_hosts_line(line: &str) -> Option<&str> {
    let mut parts = line.split_whitespace();
    let ip = parts.next()?;
    if ip == "0.0.0.0" || ip == "127.0.0.1" || ip == "::" || ip == "::1" {
        let host = parts.next()?;
        // reject "localhost" entries and anything with illegal chars
        if host == "localhost" || host.contains('/') {
            return None;
        }
        return Some(host);
    }
    None
}

/// Parse a simple ABP domain anchor rule with no path/options, like `||ads.example.com^`.
fn parse_simple_domain_anchor(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("||")?;
    // must end with ^ and contain no path/query/options/wildcards
    let host = rest.strip_suffix('^').unwrap_or(rest);
    if host.contains(['/', '*', '?', '$', '|', '^']) || host.is_empty() {
        return None;
    }
    // require it to look like a domain (has a dot, valid chars)
    if !host.contains('.') {
        return None;
    }
    if host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        Some(host)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_set_matches_subdomains() {
        let mut ds = DomainSet::new();
        ds.insert("doubleclick.net");
        assert!(ds.contains("doubleclick.net"));
        assert!(ds.contains("ads.g.doubleclick.net"));
        assert!(ds.contains("DoubleClick.NET")); // case-insensitive
        assert!(ds.contains("doubleclick.net.")); // trailing dot
        assert!(!ds.contains("notdoubleclick.net"));
        assert!(!ds.contains("example.com"));
    }

    #[test]
    fn parses_hosts_and_abp_lists() {
        let hosts = "# a hosts file\n0.0.0.0 ads.tracker.com\n127.0.0.1 metrics.evil.com\n0.0.0.0 localhost\n".to_string();
        let abp = "! EasyList sample\n||doubleclick.net^\n/pagead/*\n".to_string();
        let bl = Blocklist::from_lists(&[hosts, abp]);

        assert!(bl.is_blocked_domain("ads.tracker.com"));
        assert!(bl.is_blocked_domain("sub.metrics.evil.com"));
        assert!(bl.is_blocked_domain("doubleclick.net"));
        assert!(!bl.is_blocked_domain("localhost"));
        assert!(!bl.is_blocked_domain("example.com"));
    }

    #[test]
    fn blocks_url_level_request() {
        let abp = "||doubleclick.net^\n/pagead/*\n".to_string();
        let bl = Blocklist::from_lists(&[abp]);
        assert!(bl.is_blocked_request(
            "https://doubleclick.net/pagead/ads.js",
            "https://news.example",
            "script"
        ));
        assert!(bl.is_blocked_request(
            "https://cdn.example/pagead/track.gif",
            "https://news.example",
            "image"
        ));
        assert!(!bl.is_blocked_request(
            "https://news.example/article.html",
            "https://news.example",
            "document"
        ));
    }
}
