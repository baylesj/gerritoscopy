//! Well-known Gerrit host aliases and resolution logic.

use anyhow::{bail, Result};

// ---------------------------------------------------------------------------
// Known hosts
// ---------------------------------------------------------------------------

/// Short alias → canonical base URL for well-known public Gerrit instances.
pub const KNOWN_HOSTS: &[(&str, &str)] = &[
    ("chromium", "https://chromium-review.googlesource.com"),
    ("android", "https://android-review.googlesource.com"),
    ("go", "https://go-review.googlesource.com"),
    ("fuchsia", "https://fuchsia-review.googlesource.com"),
    ("skia", "https://skia-review.googlesource.com"),
    ("gerrit", "https://gerrit-review.googlesource.com"),
    ("wikimedia", "https://gerrit.wikimedia.org"),
    ("qt", "https://codereview.qt-project.org"),
    ("libreoffice", "https://gerrit.libreoffice.org"),
    ("onap", "https://gerrit.onap.org"),
    ("webrtc", "https://webrtc-review.googlesource.com"),
];

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

/// Resolve a single host token to `(alias, canonical_url)`.
///
/// - **Short alias** → `("chromium", "https://chromium-review.googlesource.com")`
/// - **Full URL** → derived alias (known short name if the URL matches the
///   table, otherwise the hostname) plus the URL with any trailing `/` stripped.
///
/// Returns an error for unrecognised short names (non-URL tokens not in the table).
pub fn resolve(s: &str) -> Result<(String, String)> {
    let s = s.trim();

    if s.starts_with("http://") || s.starts_with("https://") {
        let url = s.trim_end_matches('/').to_owned();
        let alias = KNOWN_HOSTS
            .iter()
            .find(|(_, u)| *u == url.as_str())
            .map(|(a, _)| (*a).to_owned())
            .unwrap_or_else(|| {
                // Fall back to the hostname portion of the URL.
                url.trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .split('/')
                    .next()
                    .unwrap_or(&url)
                    .to_owned()
            });
        return Ok((alias, url));
    }

    match KNOWN_HOSTS.iter().find(|(a, _)| *a == s) {
        Some((alias, url)) => Ok((alias.to_string(), url.to_string())),
        None => {
            let known = KNOWN_HOSTS
                .iter()
                .map(|(a, _)| *a)
                .collect::<Vec<_>>()
                .join(", ");
            bail!("unknown host {s:?}; pass a full URL or one of: {known}")
        }
    }
}

/// Expand a list of `--host` values into `(alias, url)` pairs.
///
/// Each element may be:
/// - a single token: `"chromium"` or `"https://my-gerrit.corp.com"`
/// - comma-separated tokens: `"chromium,go,android"`
///
/// Duplicate URLs are silently dropped (last-one-wins for the alias).
pub fn expand(specs: &[String]) -> Result<Vec<(String, String)>> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for spec in specs {
        for token in spec.split(',') {
            let (alias, url) = resolve(token.trim())?;
            if seen.insert(url.clone()) {
                out.push((alias, url));
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_known_alias() {
        let (alias, url) = resolve("chromium").unwrap();
        assert_eq!(alias, "chromium");
        assert_eq!(url, "https://chromium-review.googlesource.com");
    }

    #[test]
    fn resolve_full_url_unknown() {
        let (alias, url) = resolve("https://my-gerrit.corp.com").unwrap();
        assert_eq!(alias, "my-gerrit.corp.com");
        assert_eq!(url, "https://my-gerrit.corp.com");
    }

    #[test]
    fn resolve_full_url_strips_trailing_slash() {
        let (_, url) = resolve("https://my-gerrit.corp.com/").unwrap();
        assert_eq!(url, "https://my-gerrit.corp.com");
    }

    #[test]
    fn resolve_full_url_known_returns_short_alias() {
        // Passing the full URL of a known host should still give the short alias.
        let (alias, _) = resolve("https://chromium-review.googlesource.com").unwrap();
        assert_eq!(alias, "chromium");
    }

    #[test]
    fn resolve_unknown_alias_errors() {
        assert!(resolve("notahost").is_err());
        let err = resolve("notahost").unwrap_err().to_string();
        assert!(err.contains("notahost"));
        assert!(err.contains("chromium"), "error should list known aliases");
    }

    #[test]
    fn expand_single() {
        let hosts = expand(&["chromium".to_owned()]).unwrap();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].0, "chromium");
    }

    #[test]
    fn expand_comma_separated() {
        let hosts = expand(&["chromium,go".to_owned()]).unwrap();
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].0, "chromium");
        assert_eq!(hosts[1].0, "go");
    }

    #[test]
    fn expand_multiple_flags() {
        let hosts = expand(&["chromium".to_owned(), "go".to_owned()]).unwrap();
        assert_eq!(hosts.len(), 2);
    }

    #[test]
    fn expand_deduplicates_by_url() {
        // Same URL, two ways of specifying it.
        let hosts = expand(&[
            "chromium".to_owned(),
            "https://chromium-review.googlesource.com".to_owned(),
        ])
        .unwrap();
        assert_eq!(hosts.len(), 1);
    }

    #[test]
    fn expand_empty_defaults_to_nothing() {
        // The caller (main) provides the default; expand itself doesn't inject one.
        assert!(expand(&[]).unwrap().is_empty());
    }

    #[test]
    fn expand_propagates_unknown_alias_error() {
        assert!(expand(&["chromium,badhost".to_owned()]).is_err());
    }
}
