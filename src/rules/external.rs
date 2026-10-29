//! The external link rule, `MADR11`.
//!
//! Opt-in by design: it needs network access, and a remote server's answer can change between runs,
//! so a finding is a warning unless `[rules] MADR11 = "error"` upgrades it. Destinations come from the
//! parsed [`Document`] links and images, never from the source text.
//!
//! The rule runs in two steps so a URL cited by several ADRs is requested once per run:
//! [`external_citations`] lists one document's external links without touching the network, and
//! [`check_external_links`] requests every unique URL across all citations, on a bounded number of
//! threads. `HEAD` is tried first; a server that refuses `HEAD` (`403`, `405`, `501`) is asked again
//! with `GET`, whose body is never read. A `2xx` or `3xx` status after redirects is a working link;
//! `429 Too Many Requests` is not reported, because rate limiting says nothing about whether the page
//! exists.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;
use std::time::Duration;

use crate::adr::Document;
use crate::diagnostics::Diagnostic;
use crate::parallel;

/// How long one URL may take, redirects included.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Redirects followed before a URL is reported as broken.
const MAX_REDIRECTS: u32 = 5;

/// Requests in flight at once, across every host. A fixed politeness bound rather than the core
/// count: the work waits on remote servers, not the CPU.
const MAX_CONCURRENT_REQUESTS: usize = 8;

/// Statuses that mean "this server does not answer `HEAD`", so the URL is retried with `GET`.
const GET_FALLBACK_STATUSES: [u16; 3] = [403, 405, 501];

/// Rate limiting is not evidence of a broken link.
const TOO_MANY_REQUESTS: u16 = 429;

/// What a request found out about one URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The server answered with a `2xx` or `3xx` status.
    Ok,
    /// The server rate-limited the request; nothing is reported.
    RateLimited,
    /// The server answered with a `4xx` or `5xx` status.
    Status(u16),
    /// The request failed before a status arrived: DNS, TLS, timeout, too many redirects.
    Failed(String),
}

/// One place an ADR cites an external URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Citation {
    /// The URL to request: `http(s)`, without the fragment.
    pub url: String,
    /// The `MADR11` warning to report when the URL is broken, located at the citing line. Its message
    /// is set by [`check_external_links`] from the outcome.
    pub finding: Diagnostic,
}

/// The URL to request for a link destination, or `None` when it is not an `http(s)` link.
///
/// A protocol-relative `//host/path` is requested over `https`. The fragment is dropped: it is never
/// sent to the server, so `page#a` and `page#b` are one request.
pub fn external_url(destination: &str) -> Option<String> {
    let destination = destination.trim();
    let url = if destination.starts_with("//") {
        format!("https:{destination}")
    } else if has_scheme(destination, "http") || has_scheme(destination, "https") {
        destination.to_owned()
    } else {
        return None;
    };
    let end = url.find('#').unwrap_or(url.len());
    let url = &url[..end];
    // `https://` with no host is not something a request can be made for; report nothing rather than
    // a confusing network error.
    let host = url.split_once("//").map_or("", |(_, rest)| rest);
    if host.is_empty() || host.starts_with('/') {
        return None;
    }
    Some(url.to_owned())
}

fn has_scheme(url: &str, scheme: &str) -> bool {
    url.get(..scheme.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
        && url[scheme.len()..].starts_with("://")
}

/// Every external link and image in `document`, once per URL and line. Makes no request.
///
/// The findings carry only their line, as the other link rules' do; the caller locates them against
/// the source with [`crate::diagnostics::locate`].
pub fn external_citations(document: &Document, path: &Path) -> Vec<Citation> {
    let mut seen = BTreeSet::new();
    document
        .links
        .iter()
        .filter_map(|link| external_url(&link.url).map(|url| (url, link.line)))
        .filter(|(url, line)| seen.insert((url.clone(), *line)))
        .map(|(url, line)| Citation {
            finding: Diagnostic::warning("MADR11", path, line, url.clone()),
            url,
        })
        .collect()
}

/// Classifies a final HTTP status. Redirects have already been followed by the client.
pub fn classify_status(status: u16) -> Outcome {
    match status {
        200..=399 => Outcome::Ok,
        TOO_MANY_REQUESTS => Outcome::RateLimited,
        _ => Outcome::Status(status),
    }
}

/// Requests every unique URL in `citations` once and returns the findings of the citations whose
/// URL is broken, in citation order.
///
/// Findings are warnings; `[rules] MADR11` sets their severity like any other code.
///
/// # Errors
///
/// Returns the operating system error when a worker thread cannot be spawned.
pub fn check_external_links(citations: Vec<Citation>, timeout: Duration) -> io::Result<Vec<Diagnostic>> {
    let agent = agent(timeout);
    check_with(citations, |url| probe(&agent, url))
}

/// [`check_external_links`] with the network replaced by `probe`, which is called once per URL.
fn check_with(citations: Vec<Citation>, probe: impl Fn(&str) -> Outcome + Sync) -> io::Result<Vec<Diagnostic>> {
    let urls = citations
        .iter()
        .map(|citation| citation.url.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let outcomes = parallel::map_on(&urls, MAX_CONCURRENT_REQUESTS, |url: &String| probe(url))?;
    let outcomes = urls.into_iter().zip(outcomes).collect::<BTreeMap<_, _>>();

    let mut diagnostics = Vec::new();
    for Citation { url, mut finding } in citations {
        finding.message = match &outcomes[&url] {
            Outcome::Ok | Outcome::RateLimited => continue,
            Outcome::Status(status) => format!("external link '{url}' returned HTTP {status}"),
            Outcome::Failed(error) => format!("external link '{url}' could not be fetched: {error}"),
        };
        diagnostics.push(finding);
    }
    Ok(diagnostics)
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .max_redirects(MAX_REDIRECTS)
        .http_status_as_error(false)
        .user_agent(concat!("adrlint/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// `HEAD` first, then `GET` when the server refuses `HEAD`. The `GET` body is never read.
fn probe(agent: &ureq::Agent, url: &str) -> Outcome {
    let status = match agent.head(url).call() {
        Ok(response) => response.status().as_u16(),
        Err(error) => return Outcome::Failed(error.to_string()),
    };
    if !GET_FALLBACK_STATUSES.contains(&status) {
        return classify_status(status);
    }
    match agent.get(url).call() {
        Ok(response) => classify_status(response.status().as_u16()),
        Err(error) => Outcome::Failed(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adr::parse;
    use crate::diagnostics::Severity;
    use std::sync::Mutex;

    #[test]
    fn should_select_only_http_and_https_destinations() {
        assert_eq!(
            external_url("https://example.com/a"),
            Some("https://example.com/a".to_owned())
        );
        assert_eq!(
            external_url("http://example.com"),
            Some("http://example.com".to_owned())
        );
        assert_eq!(
            external_url("HTTPS://Example.com"),
            Some("HTTPS://Example.com".to_owned())
        );
        assert_eq!(external_url("mailto:team@example.com"), None);
        assert_eq!(external_url("ftp://example.com/file"), None);
        assert_eq!(external_url("data:text/plain,hi"), None);
        assert_eq!(external_url("file:///etc/passwd"), None);
        assert_eq!(external_url("#decision-outcome"), None);
        assert_eq!(external_url("/docs/adr/0001.md"), None);
        assert_eq!(external_url("docs/http://not-a-url"), None);
        assert_eq!(external_url("httpx://example.com"), None);
        assert_eq!(external_url("https://"), None);
        assert_eq!(external_url("https:///path"), None);
    }

    #[test]
    fn should_request_protocol_relative_links_over_https() {
        assert_eq!(
            external_url("//example.com/x"),
            Some("https://example.com/x".to_owned())
        );
        assert_eq!(external_url("///path"), None);
    }

    #[test]
    fn should_drop_the_fragment() {
        assert_eq!(
            external_url("https://example.com/page?q=1#section"),
            Some("https://example.com/page?q=1".to_owned())
        );
    }

    #[test]
    fn should_classify_statuses() {
        assert_eq!(classify_status(200), Outcome::Ok);
        assert_eq!(classify_status(204), Outcome::Ok);
        assert_eq!(classify_status(304), Outcome::Ok);
        assert_eq!(classify_status(404), Outcome::Status(404));
        assert_eq!(classify_status(410), Outcome::Status(410));
        assert_eq!(classify_status(500), Outcome::Status(500));
        assert_eq!(classify_status(429), Outcome::RateLimited);
    }

    fn citations(documents: &[(&str, &str)]) -> Vec<Citation> {
        documents
            .iter()
            .flat_map(|(path, source)| external_citations(&parse(source), Path::new(path)))
            .collect()
    }

    #[test]
    fn should_cite_links_and_images_once_per_url_and_line() {
        let found = citations(&[(
            "0001-a.md",
            "# T\n\n[a](https://example.com/a) [again](https://example.com/a#x)\n\n\
             ![img](https://example.com/i.png)\n\n[mail](mailto:x@example.com) [b](//example.com/b)\n",
        )])
        .into_iter()
        .map(|citation| (citation.url, citation.finding.line, citation.finding.severity))
        .collect::<Vec<_>>();
        assert_eq!(
            found,
            [
                ("https://example.com/a".to_owned(), 3, Severity::Warning),
                ("https://example.com/i.png".to_owned(), 5, Severity::Warning),
                ("https://example.com/b".to_owned(), 7, Severity::Warning),
            ]
        );
    }

    #[test]
    fn should_probe_each_url_once_and_report_every_location() {
        let cited = citations(&[
            (
                "0001-a.md",
                "# T\n\n[a](https://example.com/gone)\n\n[b](https://example.com/ok)\n",
            ),
            (
                "0002-b.md",
                "# U\n\n[a](https://example.com/gone)\n\n[c](https://example.com/slow)\n",
            ),
        ]);
        let calls = Mutex::new(Vec::new());
        let diagnostics = check_with(cited, |url| {
            calls.lock().unwrap().push(url.to_owned());
            match url {
                "https://example.com/gone" => Outcome::Status(404),
                "https://example.com/slow" => Outcome::Failed("timeout: Global".to_owned()),
                _ => Outcome::Ok,
            }
        })
        .unwrap();

        let mut calls = calls.into_inner().unwrap();
        calls.sort();
        assert_eq!(
            calls,
            [
                "https://example.com/gone",
                "https://example.com/ok",
                "https://example.com/slow"
            ]
        );

        let found = diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.code,
                    diagnostic.severity,
                    diagnostic.path.to_string_lossy().into_owned(),
                    diagnostic.line,
                    diagnostic.message.as_str(),
                    diagnostic.fixable,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            found,
            [
                (
                    "MADR11",
                    Severity::Warning,
                    "0001-a.md".to_owned(),
                    3,
                    "external link 'https://example.com/gone' returned HTTP 404",
                    false
                ),
                (
                    "MADR11",
                    Severity::Warning,
                    "0002-b.md".to_owned(),
                    3,
                    "external link 'https://example.com/gone' returned HTTP 404",
                    false
                ),
                (
                    "MADR11",
                    Severity::Warning,
                    "0002-b.md".to_owned(),
                    5,
                    "external link 'https://example.com/slow' could not be fetched: timeout: Global",
                    false
                ),
            ]
        );
    }

    #[test]
    fn should_not_report_a_rate_limited_url() {
        let cited = citations(&[("0001-a.md", "# T\n\n[a](https://example.com/busy)\n")]);
        let diagnostics = check_with(cited, |_| Outcome::RateLimited).unwrap();
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn should_make_no_request_without_external_links() {
        let cited = citations(&[(
            "0001-a.md",
            "# T\n\n[local](0002-b.md) [mail](mailto:x@example.com) [anchor](#t)\n",
        )]);
        assert!(cited.is_empty());
        let diagnostics = check_with(cited, |url| panic!("unexpected request for {url}")).unwrap();
        assert!(diagnostics.is_empty());
    }
}
