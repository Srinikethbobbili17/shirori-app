//! A careful HTTP fetcher (§8.1 "validate URLs and prevent SSRF").
//!
//! - Only the source's allow-listed host (or its subdomains) may be contacted.
//! - Every hop's DNS answer is checked: private, loopback, link-local and similar ranges are
//!   blocked, and the connection is pinned to the checked addresses (no DNS rebinding).
//! - Redirects are followed manually (max 3), re-checking each hop.
//! - Bodies are size-limited, requests time out, and conditional GET saves bandwidth.

use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use reqwest::{StatusCode, header};
use url::Url;

use crate::config::IngestConfig;

pub const MAX_REDIRECTS: usize = 3;

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("blocked: {0}")]
    Blocked(String),
    #[error("disallowed by robots.txt")]
    Robots,
    #[error("HTTP status {0}")]
    Status(u16),
    #[error("response larger than {0} bytes")]
    TooLarge(usize),
    #[error("too many redirects")]
    TooManyRedirects,
    #[error("network error: {0}")]
    Network(String),
}

impl FetchError {
    pub fn code(&self) -> String {
        match self {
            Self::Blocked(_) => "blocked".into(),
            Self::Robots => "robots_disallowed".into(),
            Self::Status(s) => format!("http_{s}"),
            Self::TooLarge(_) => "too_large".into(),
            Self::TooManyRedirects => "too_many_redirects".into(),
            Self::Network(_) => "network".into(),
        }
    }

    /// Transient problems are retried; policy blocks are not.
    pub fn retryable(&self) -> bool {
        match self {
            Self::Status(s) => *s == 408 || *s == 429 || *s >= 500,
            Self::Network(_) => true,
            _ => false,
        }
    }
}

#[derive(Debug)]
pub enum Fetched {
    NotModified,
    Body {
        bytes: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
        final_url: Url,
    },
}

#[derive(Clone)]
pub struct Fetcher {
    allow_private: bool,
    user_agent: String,
    timeout: Duration,
    max_bytes: usize,
}

impl Fetcher {
    pub fn new(cfg: &IngestConfig) -> Self {
        Self {
            allow_private: cfg.allow_private_networks,
            user_agent: cfg.user_agent.clone(),
            timeout: Duration::from_secs(cfg.request_timeout_secs),
            max_bytes: cfg.max_feed_bytes,
        }
    }

    pub async fn fetch(
        &self,
        url: &Url,
        allowed_host: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<Fetched, FetchError> {
        let mut current = url.clone();
        for _ in 0..=MAX_REDIRECTS {
            let addrs = self.check_target(&current, allowed_host).await?;
            let client = self.client_for(&current, &addrs)?;
            let mut req = client
                .get(current.clone())
                .header(header::USER_AGENT, &self.user_agent)
                .header(
                    header::ACCEPT,
                    "application/rss+xml, application/atom+xml, application/feed+json, \
                     application/xml;q=0.9, text/xml;q=0.8, text/plain;q=0.5",
                );
            if let Some(e) = etag {
                req = req.header(header::IF_NONE_MATCH, e);
            }
            if let Some(lm) = last_modified {
                req = req.header(header::IF_MODIFIED_SINCE, lm);
            }
            let mut resp = req.send().await.map_err(|e| FetchError::Network(e.to_string()))?;
            let status = resp.status();
            if status == StatusCode::NOT_MODIFIED {
                return Ok(Fetched::NotModified);
            }
            if status.is_redirection() {
                let location = resp
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .ok_or(FetchError::Status(status.as_u16()))?;
                current = current
                    .join(location)
                    .map_err(|_| FetchError::Blocked("invalid redirect location".into()))?;
                continue;
            }
            if !status.is_success() {
                return Err(FetchError::Status(status.as_u16()));
            }
            let header_value = |name| {
                resp.headers()
                    .get(name)
                    .and_then(|v: &header::HeaderValue| v.to_str().ok())
                    .map(str::to_string)
            };
            let new_etag = header_value(header::ETAG);
            let new_last_modified = header_value(header::LAST_MODIFIED);
            if resp.content_length().is_some_and(|n| n as usize > self.max_bytes) {
                return Err(FetchError::TooLarge(self.max_bytes));
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = resp.chunk().await.map_err(|e| FetchError::Network(e.to_string()))? {
                if bytes.len() + chunk.len() > self.max_bytes {
                    return Err(FetchError::TooLarge(self.max_bytes));
                }
                bytes.extend_from_slice(&chunk);
            }
            return Ok(Fetched::Body {
                bytes,
                etag: new_etag,
                last_modified: new_last_modified,
                final_url: current,
            });
        }
        Err(FetchError::TooManyRedirects)
    }

    /// Validate scheme and host, resolve DNS, and refuse non-public addresses.
    async fn check_target(&self, url: &Url, allowed_host: &str) -> Result<Vec<SocketAddr>, FetchError> {
        if url.scheme() != "https" && url.scheme() != "http" {
            return Err(FetchError::Blocked(format!("scheme `{}` not allowed", url.scheme())));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(FetchError::Blocked("credentials in URL are not allowed".into()));
        }
        let host = url
            .host_str()
            .ok_or_else(|| FetchError::Blocked("URL has no host".into()))?
            .to_ascii_lowercase();
        if !host_allowed(&host, allowed_host) {
            return Err(FetchError::Blocked(format!(
                "host `{host}` is not the source's allowed host `{allowed_host}`"
            )));
        }
        let port = url.port_or_known_default().unwrap_or(443);
        let lookup_host = host.trim_start_matches('[').trim_end_matches(']').to_string();
        let addrs: Vec<SocketAddr> = tokio::net::lookup_host((lookup_host.as_str(), port))
            .await
            .map_err(|e| FetchError::Network(format!("DNS lookup failed for {host}: {e}")))?
            .collect();
        if addrs.is_empty() {
            return Err(FetchError::Network(format!("no addresses for {host}")));
        }
        if !self.allow_private
            && let Some(bad) = addrs.iter().find(|a| !is_public_ip(a.ip()))
        {
            return Err(FetchError::Blocked(format!(
                "{host} resolves to non-public address {}",
                bad.ip()
            )));
        }
        Ok(addrs)
    }

    fn client_for(&self, url: &Url, addrs: &[SocketAddr]) -> Result<reqwest::Client, FetchError> {
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(self.timeout)
            .connect_timeout(Duration::from_secs(10));
        if let Some(domain) = url.domain() {
            builder = builder.resolve_to_addrs(domain, addrs);
        }
        builder.build().map_err(|e| FetchError::Network(e.to_string()))
    }
}

/// `host` equals `allowed` or is a subdomain of it.
pub fn host_allowed(host: &str, allowed: &str) -> bool {
    let allowed = allowed.to_ascii_lowercase();
    host == allowed || host.ends_with(&format!(".{allowed}"))
}

/// True only for globally routable unicast addresses.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_multicast()
                || o[0] == 0
                || (o[0] == 100 && (64..128).contains(&o[1])) // shared address space (CGNAT)
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)     // IETF protocol assignments
                || (o[0] == 198 && (18..20).contains(&o[1]))  // benchmarking
                || o[0] >= 240) // reserved
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.segments()[0] == 0x2001 && v6.segments()[1] == 0x0db8) // documentation
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_private_and_special_addresses() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip} should be blocked");
        }
        for ip in ["93.184.216.34", "1.1.1.1", "2606:4700:4700::1111"] {
            assert!(is_public_ip(ip.parse().unwrap()), "{ip} should be allowed");
        }
    }

    #[test]
    fn host_allow_list() {
        assert!(host_allowed("example.org", "example.org"));
        assert!(host_allowed("feeds.example.org", "example.org"));
        assert!(!host_allowed("evilexample.org", "example.org"));
        assert!(!host_allowed("example.org.evil.com", "example.org"));
    }
}
