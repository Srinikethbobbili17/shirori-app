//! Feed parsing: RSS 0.9x/1.0/2.0, Atom and JSON Feed via `feed-rs`, normalised to items.
//! Feed content is untrusted data (§8.1); it is only ever stored as plain text.

use chrono::{DateTime, Utc};
use url::Url;

use crate::canonical::{canonical_url, content_hash, extract_doi, strip_html};

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedItem {
    pub external_id: String,
    pub canonical_url: Option<String>,
    pub doi: Option<String>,
    pub title: String,
    pub authors: Option<String>,
    /// Plain-text summary. Callers decide whether the source allows keeping it.
    pub summary: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub content_hash: String,
}

pub fn parse_feed(bytes: &[u8], base: &Url, max_items: usize) -> Result<Vec<ParsedItem>, String> {
    let feed = feed_rs::parser::parse(bytes).map_err(|e| format!("could not parse feed: {e}"))?;
    let mut items = Vec::new();
    for entry in feed.entries.into_iter().take(max_items) {
        let Some(title) = entry.title.map(|t| strip_html(&t.content)).filter(|t| !t.is_empty()) else {
            continue; // an item with no title can't become a card
        };
        let link = entry
            .links
            .iter()
            .find(|l| l.rel.as_deref().is_none_or(|r| r == "alternate"))
            .or_else(|| entry.links.first())
            .and_then(|l| canonical_url(&l.href, Some(base)));
        let summary = entry
            .summary
            .map(|s| strip_html(&s.content))
            .or_else(|| entry.content.and_then(|c| c.body).map(|b| strip_html(&b)));
        let doi = link
            .as_deref()
            .and_then(|l| l.strip_prefix("https://doi.org/").map(str::to_string))
            .or_else(|| extract_doi(&entry.id))
            .or_else(|| entry.links.iter().find_map(|l| extract_doi(&l.href)))
            .or_else(|| summary.as_deref().and_then(extract_doi));
        let authors: Vec<String> = entry
            .authors
            .iter()
            .map(|p| p.name.trim().to_string())
            .filter(|n| !n.is_empty())
            .collect();
        let summary = summary
            .map(|s| strip_byline(&s, &authors).to_string())
            .filter(|s| !s.is_empty());
        let external_id = if entry.id.trim().is_empty() {
            link.clone().unwrap_or_else(|| title.clone())
        } else {
            entry.id.trim().to_string()
        };
        items.push(ParsedItem {
            content_hash: content_hash(&title, link.as_deref(), summary.as_deref()),
            external_id,
            canonical_url: link,
            doi,
            title,
            authors: (!authors.is_empty()).then(|| authors.join(", ")),
            summary,
            published_at: entry.published.or(entry.updated),
        });
    }
    Ok(items)
}

/// Some publishers (PLOS, for one) start the summary with "by <author list>", which repeats
/// the authors field. Notices such as corrections often have nothing else, so this can
/// leave an empty summary.
fn strip_byline<'a>(summary: &'a str, authors: &[String]) -> &'a str {
    if authors.is_empty() {
        return summary;
    }
    let byline = authors.join(", ");
    ["by ", "By "]
        .iter()
        .find_map(|p| summary.strip_prefix(p)?.strip_prefix(byline.as_str()))
        .map_or(summary, |rest| rest.trim_start_matches([' ', ',', '.', ';', ':']))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel><title>Example</title>
  <item>
    <title>Wolves &amp; rivers</title>
    <link>https://news.example.org/wolves?utm_source=rss</link>
    <guid>wolves-1</guid>
    <description>&lt;p&gt;New study, doi:10.1016/j.biocon.2011.11.005&lt;/p&gt;</description>
    <pubDate>Tue, 05 Nov 2024 10:00:00 GMT</pubDate>
  </item>
  <item><description>No title here</description></item>
</channel></rss>"#;

    const ATOM: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"><title>Journal</title><id>urn:j</id>
  <updated>2024-01-01T00:00:00Z</updated>
  <entry>
    <title>Sleep and memory</title>
    <id>urn:paper:1</id>
    <link rel="alternate" href="https://doi.org/10.1038/NRN2762"/>
    <author><name>S. Diekelmann</name></author><author><name>J. Born</name></author>
    <updated>2024-01-02T00:00:00Z</updated>
    <summary>A review.</summary>
  </entry>
</feed>"#;

    #[test]
    fn parses_rss_items() {
        let base = Url::parse("https://news.example.org/feed").unwrap();
        let items = parse_feed(RSS.as_bytes(), &base, 10).unwrap();
        assert_eq!(items.len(), 1, "untitled item is skipped");
        let item = &items[0];
        assert_eq!(item.title, "Wolves & rivers");
        assert_eq!(item.external_id, "wolves-1");
        assert_eq!(item.canonical_url.as_deref(), Some("https://news.example.org/wolves"));
        assert_eq!(item.doi.as_deref(), Some("10.1016/j.biocon.2011.11.005"));
        assert!(item.summary.as_deref().unwrap().starts_with("New study"));
        assert!(item.published_at.is_some());
    }

    #[test]
    fn parses_atom_entries() {
        let base = Url::parse("https://journal.example.org/atom").unwrap();
        let items = parse_feed(ATOM.as_bytes(), &base, 10).unwrap();
        let item = &items[0];
        assert_eq!(item.doi.as_deref(), Some("10.1038/nrn2762"));
        assert_eq!(item.canonical_url.as_deref(), Some("https://doi.org/10.1038/nrn2762"));
        assert_eq!(item.authors.as_deref(), Some("S. Diekelmann, J. Born"));
    }

    #[test]
    fn strips_a_byline_that_repeats_the_authors() {
        let authors = vec!["Hela Ghali".to_string(), "Bruno Pavy".to_string()];
        assert_eq!(
            strip_byline("by Hela Ghali, Bruno Pavy Background CVD is common.", &authors),
            "Background CVD is common."
        );
        assert_eq!(strip_byline("by Hela Ghali, Bruno Pavy", &authors), "");
        // Only an exact byline is removed.
        assert_eq!(strip_byline("by Someone Else. Text", &authors), "by Someone Else. Text");
        assert_eq!(strip_byline("by the river", &[]), "by the river");
    }

    #[test]
    fn rejects_garbage() {
        let base = Url::parse("https://x.example").unwrap();
        assert!(parse_feed(b"<html>not a feed", &base, 10).is_err());
    }

    #[test]
    fn respects_item_limit() {
        let base = Url::parse("https://news.example.org/feed").unwrap();
        let mut xml = String::from("<rss version=\"2.0\"><channel><title>t</title>");
        for i in 0..10 {
            xml.push_str(&format!("<item><title>T{i}</title><guid>{i}</guid></item>"));
        }
        xml.push_str("</channel></rss>");
        assert_eq!(parse_feed(xml.as_bytes(), &base, 3).unwrap().len(), 3);
    }
}
