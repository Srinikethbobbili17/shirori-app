//! Minimal robots.txt support (§3.2: "respect robots/ToS/licence").
//!
//! Picks the most specific group for our user-agent token (or `*`), then applies the
//! longest-matching `Allow`/`Disallow` rule. `*` wildcards and `$` anchors are supported.

/// One `User-agent` group: the agent tokens it names, and its `(is_allow, pattern)` rules.
type Group = (Vec<String>, Vec<(bool, String)>);

/// Is `path` (including any query string) allowed for `agent_token` (e.g. "shiroribot")?
pub fn is_allowed(robots_txt: &str, agent_token: &str, path: &str) -> bool {
    let agent_token = agent_token.to_ascii_lowercase();
    let mut groups: Vec<Group> = Vec::new();
    let mut last_was_agent = false;

    for line in robots_txt.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "user-agent" => {
                if !last_was_agent || groups.is_empty() {
                    groups.push((Vec::new(), Vec::new()));
                }
                groups.last_mut().unwrap().0.push(value.to_ascii_lowercase());
                last_was_agent = true;
            }
            "allow" | "disallow" => {
                last_was_agent = false;
                if let Some(group) = groups.last_mut()
                    && !value.is_empty()
                {
                    group.1.push((key == "allow", value.to_string()));
                }
            }
            _ => last_was_agent = false,
        }
    }

    let specific = groups
        .iter()
        .filter(|(agents, _)| agents.iter().any(|a| a != "*" && agent_token.contains(a.as_str())))
        .flat_map(|(_, rules)| rules.iter())
        .collect::<Vec<_>>();
    let rules = if specific.is_empty() {
        groups
            .iter()
            .filter(|(agents, _)| agents.iter().any(|a| a == "*"))
            .flat_map(|(_, rules)| rules.iter())
            .collect()
    } else {
        specific
    };

    let mut best: Option<(usize, bool)> = None;
    for (allow, pattern) in rules {
        if matches(pattern, path) {
            let len = pattern.len();
            // Longest match wins; on a tie, Allow wins.
            if best.is_none_or(|(l, a)| len > l || (len == l && *allow && !a)) {
                best = Some((len, *allow));
            }
        }
    }
    best.is_none_or(|(_, allow)| allow)
}

fn matches(pattern: &str, path: &str) -> bool {
    let (pattern, anchored) = match pattern.strip_suffix('$') {
        Some(p) => (p, true),
        None => (pattern, false),
    };
    let parts: Vec<&str> = pattern.split('*').collect();
    let first = parts[0];
    if !path.starts_with(first) {
        return false;
    }
    let mut pos = first.len();
    if parts.len() == 1 {
        return !anchored || pos == path.len();
    }
    let last = parts[parts.len() - 1];
    for part in &parts[1..parts.len() - 1] {
        match path[pos..].find(part) {
            Some(found) => pos += found + part.len(),
            None => return false,
        }
    }
    if anchored {
        path.len() >= pos + last.len() && path.ends_with(last)
    } else {
        path[pos..].contains(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROBOTS: &str = "
User-agent: *
Disallow: /private/
Allow: /private/feed.xml

User-agent: BadBot
Disallow: /

User-agent: ShiroriBot
Disallow: /no-shirori
";

    #[test]
    fn specific_group_beats_wildcard() {
        assert!(!is_allowed(ROBOTS, "ShiroriBot", "/no-shirori/feed"));
        // The specific group replaces '*', so /private/ is not blocked for us.
        assert!(is_allowed(ROBOTS, "ShiroriBot", "/private/x"));
    }

    #[test]
    fn wildcard_group_with_longest_match() {
        assert!(!is_allowed(ROBOTS, "OtherBot", "/private/x"));
        assert!(is_allowed(ROBOTS, "OtherBot", "/private/feed.xml"));
        assert!(is_allowed(ROBOTS, "OtherBot", "/public"));
    }

    #[test]
    fn empty_or_missing_rules_allow_everything() {
        assert!(is_allowed("", "ShiroriBot", "/anything"));
        assert!(is_allowed("User-agent: *\nDisallow:\n", "ShiroriBot", "/anything"));
    }

    #[test]
    fn wildcards_and_anchors() {
        let robots = "User-agent: *\nDisallow: /*.xml$\n";
        assert!(!is_allowed(robots, "ShiroriBot", "/feed.xml"));
        assert!(is_allowed(robots, "ShiroriBot", "/feed.xml?x=1"));
    }

    #[test]
    fn anchored_wildcard_matches_suffix() {
        let robots = "User-agent: *\nDisallow: /*.xml$\n";
        assert!(!is_allowed(robots, "ShiroriBot", "/a.xml.xml"));
        assert!(!is_allowed(robots, "ShiroriBot", "/dir/feed.xml"));
    }

    #[test]
    fn disallow_all() {
        assert!(!is_allowed("User-agent: *\nDisallow: /\n", "ShiroriBot", "/feed"));
    }
}
