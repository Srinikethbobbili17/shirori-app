//! Canonical identifiers for deduplication (§11: "canonical identifiers and a duplicate review
//! workflow"), plus small text helpers.

use sha2::{Digest, Sha256};
use url::Url;

/// Query parameters that only track clicks and never change the content.
const TRACKING_PARAMS: &[&str] = &[
    "fbclid", "gclid", "dclid", "mc_cid", "mc_eid", "igshid", "ref_src", "cmpid",
];

/// Normalise a URL so the same article from two feeds compares equal:
/// http(s) only, no fragment, no tracking parameters, remaining parameters sorted.
/// `doi.org` links become `https://doi.org/<lower-case DOI>`.
pub fn canonical_url(raw: &str, base: Option<&Url>) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let mut url = match base {
        Some(b) => b.join(raw).ok()?,
        None => Url::parse(raw).ok()?,
    };
    if url.scheme() != "http" && url.scheme() != "https" {
        return None;
    }
    if let Some(doi) = doi_from_url(&url) {
        return Some(format!("https://doi.org/{doi}"));
    }
    url.set_fragment(None);
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| {
            let k = k.to_ascii_lowercase();
            !k.starts_with("utm_") && !TRACKING_PARAMS.contains(&k.as_str())
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    pairs.sort();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    Some(url.to_string())
}

fn doi_from_url(url: &Url) -> Option<String> {
    let host = url.host_str()?.to_ascii_lowercase();
    if host == "doi.org" || host == "dx.doi.org" {
        let path = percent_decode(url.path().trim_start_matches('/'));
        return extract_doi(&path);
    }
    None
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Find the first DOI (`10.<4–9 digits>/<suffix>`) in a string. Returns it lower-cased.
pub fn extract_doi(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while let Some(pos) = text[i..].find("10.") {
        let start = i + pos;
        let mut j = start + 3;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        let registrant = j - (start + 3);
        let preceded_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        if preceded_ok && (4..=9).contains(&registrant) && j < bytes.len() && bytes[j] == b'/' {
            let suffix_start = j + 1;
            let mut k = suffix_start;
            while k < bytes.len() && !bytes[k].is_ascii_whitespace() && !b"\"'<>".contains(&bytes[k]) {
                k += 1;
            }
            let doi = text[start..k].trim_end_matches(['.', ',', ';', ':', ')', ']']);
            if doi.len() > suffix_start - start {
                return Some(doi.to_ascii_lowercase());
            }
        }
        i = start + 3;
    }
    None
}

/// Turn feed HTML into plain text: drop tags, decode common entities, collapse whitespace.
pub fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Truncate to at most `max` characters on a word boundary, adding "…" if cut.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map(|(head, _)| head).unwrap_or(&cut);
    format!("{cut}…")
}

/// Fingerprint of the parts of an item that matter to readers; a change means "updated".
pub fn content_hash(title: &str, url: Option<&str>, summary: Option<&str>) -> String {
    let mut h = Sha256::new();
    for part in [title, url.unwrap_or(""), summary.unwrap_or("")] {
        h.update(part.as_bytes());
        h.update([0u8]);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tracking_and_fragments_and_sorts_params() {
        assert_eq!(
            canonical_url("https://Example.com/a?utm_source=x&b=2&a=1&fbclid=z#top", None).unwrap(),
            "https://example.com/a?a=1&b=2"
        );
        assert_eq!(
            canonical_url("https://example.com/a?utm_medium=rss", None).unwrap(),
            "https://example.com/a"
        );
    }

    #[test]
    fn resolves_relative_links_and_rejects_other_schemes() {
        let base = Url::parse("https://news.example.org/feed.xml").unwrap();
        assert_eq!(
            canonical_url("/story/1", Some(&base)).unwrap(),
            "https://news.example.org/story/1"
        );
        assert_eq!(canonical_url("javascript:alert(1)", None), None);
        assert_eq!(canonical_url("", None), None);
    }

    #[test]
    fn doi_links_are_canonical() {
        assert_eq!(
            canonical_url("http://dx.doi.org/10.1037/0033-2909.132.3.354?utm_source=x", None).unwrap(),
            "https://doi.org/10.1037/0033-2909.132.3.354"
        );
        assert_eq!(
            canonical_url("https://doi.org/10.1111/J.1467-9280.2006.01693.X", None).unwrap(),
            "https://doi.org/10.1111/j.1467-9280.2006.01693.x"
        );
    }

    #[test]
    fn extracts_dois_from_text() {
        assert_eq!(
            extract_doi("See doi:10.1038/nrn2762.").as_deref(),
            Some("10.1038/nrn2762")
        );
        assert_eq!(
            extract_doi("<a href=\"https://doi.org/10.1086/261155\">").as_deref(),
            Some("10.1086/261155")
        );
        assert_eq!(extract_doi("version 10.2 of the app"), None);
        assert_eq!(extract_doi("x210.1234/abc"), None);
    }

    #[test]
    fn decodes_percent_encoded_dois() {
        assert_eq!(
            canonical_url("https://doi.org/10.1002%2F(SICI)1097", None).unwrap(),
            "https://doi.org/10.1002/(sici)1097"
        );
    }

    #[test]
    fn strips_html() {
        assert_eq!(
            strip_html("<p>Wolves &amp; elk<br/>in <b>Yellowstone</b></p>"),
            "Wolves & elk in Yellowstone"
        );
    }

    #[test]
    fn truncates_on_word_boundary() {
        assert_eq!(truncate("one two three", 100), "one two three");
        assert_eq!(truncate("one two three", 9), "one two…");
    }

    #[test]
    fn hash_changes_with_title() {
        assert_ne!(content_hash("a", None, None), content_hash("b", None, None));
        assert_eq!(content_hash("a", Some("u"), None), content_hash("a", Some("u"), None));
    }
}
