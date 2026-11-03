//! RSS/Atom feed fetching for the RSS tab.

use crate::model::{parse_nzb, sanitize_folder, ParsedNzb, RssItem};
use std::io::Read;
use std::time::Duration;
use std::sync::Arc;

pub struct FetchResult {
    pub items: Vec<RssItem>,
    pub downloads: Vec<DownloadedNzb>,
    pub errors: Vec<String>,
}

pub struct DownloadedNzb {
    pub nzb: ParsedNzb,
    pub source: String,
    pub paused: bool,
}

#[derive(Clone, Debug)]
struct Filter {
    pattern: String,
    accept: bool,
    pause: bool,
}

pub fn fetch_and_download(
    urls: &[String],
    filters: &str,
    nzb_dir: &std::path::Path,
    existing_sources: &[String],
) -> Result<FetchResult, String> {
    let items = fetch_feeds(urls)?;
    let filters = parse_filters(filters);
    let mut downloads = Vec::new();
    let mut errors = Vec::new();
    if filters.is_empty() {
        return Ok(FetchResult {
            items,
            downloads,
            errors,
        });
    }
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS setup failed: {e}"))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .timeout(Duration::from_secs(60))
        .user_agent("NewsQueue/0.1.0")
        .build();
    let _ = std::fs::create_dir_all(nzb_dir);
    for item in &items {
        if existing_sources.iter().any(|source| source == &item.url) {
            continue;
        }
        let Some(filter) = last_matching_filter(&filters, &item.name) else {
            continue;
        };
        if !filter.accept || item.url.trim().is_empty() {
            continue;
        }
        let response = match agent.get(&item.url).call() {
            Ok(response) => response,
            Err(error) => {
                errors.push(format!("{}: {}", item.name, safe_ureq_error(error)));
                continue;
            }
        };
        let mut bytes = Vec::new();
        if let Err(error) = response.into_reader().read_to_end(&mut bytes) {
            errors.push(format!("{}: read failed: {error}", item.name));
            continue;
        }
        let path = nzb_dir.join(format!("{}.nzb", sanitize_folder(&item.name)));
        if let Err(error) = std::fs::write(&path, bytes) {
            errors.push(format!("{}: cannot save NZB: {error}", item.name));
            continue;
        }
        match parse_nzb(&path) {
            Ok(nzb) => downloads.push(DownloadedNzb {
                nzb,
                source: item.url.clone(),
                paused: filter.pause,
            }),
            Err(error) => {
                let _ = std::fs::remove_file(&path);
                errors.push(format!("{}: invalid NZB: {error}", item.name));
            }
        }
    }
    Ok(FetchResult {
        items,
        downloads,
        errors,
    })
}

fn parse_filters(text: &str) -> Vec<Filter> {
    let mut filters = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let Some((pattern, rest)) = line.split_once('|') else {
            continue;
        };
        let pattern = pattern.trim();
        if pattern.is_empty() {
            continue;
        }
        let action = rest.trim().to_ascii_uppercase();
        let accept = action.starts_with('A');
        if !accept && !action.starts_with('D') {
            continue;
        }
        filters.push(Filter {
            pattern: pattern.to_ascii_lowercase(),
            accept,
            pause: accept && action.chars().skip(1).any(|character| character == 'P'),
        });
    }
    filters
}

fn last_matching_filter<'a>(filters: &'a [Filter], name: &str) -> Option<&'a Filter> {
    filters
        .iter()
        .rev()
        .find(|filter| glob_match(&filter.pattern, &name.to_ascii_lowercase()))
}

fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let mut pi = 0;
    let mut ni = 0;
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn fetch_feeds(urls: &[String]) -> Result<Vec<RssItem>, String> {
    if urls.is_empty() {
        return Err("No RSS feeds configured".into());
    }
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| format!("TLS setup failed: {e}"))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .timeout(Duration::from_secs(30))
        .user_agent("NewsQueue/0.1.0")
        .build();
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for (index, url) in urls.iter().enumerate() {
        match fetch_one(&agent, url) {
            Ok(mut feed) => items.append(&mut feed),
            Err(error) => errors.push(format!("feed {}: {error}", index + 1)),
        }
    }
    if items.is_empty() {
        return Err(if errors.is_empty() {
            "No RSS items found".into()
        } else {
            errors.join("\n")
        });
    }
    items.truncate(500);
    Ok(items)
}

fn fetch_one(agent: &ureq::Agent, url: &str) -> Result<Vec<RssItem>, String> {
    let response = agent.get(url).call().map_err(safe_ureq_error)?;
    if !(200..300).contains(&response.status()) {
        return Err(format!("HTTP {}", response.status()));
    }
    let mut body = String::new();
    response
        .into_reader()
        .read_to_string(&mut body)
        .map_err(|e| format!("read failed: {e}"))?;
    parse_feed(&body)
}

fn safe_ureq_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => format!("HTTP {code}"),
        ureq::Error::Transport(transport) => format!("network error: {:?}", transport.kind()),
    }
}

#[derive(Clone, Copy)]
enum Field {
    Title,
    Link,
    Date,
    Description,
}

fn parse_feed(xml: &str) -> Result<Vec<RssItem>, String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut items = Vec::new();
    let mut in_item = false;
    let mut field: Option<Field> = None;
    let mut title = String::new();
    let mut link = String::new();
    let mut date = String::new();
    let mut description = String::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(event)) => {
                let name = event.name();
                match name.as_ref() {
                    b"item" | b"entry" => {
                        in_item = true;
                        title.clear();
                        link.clear();
                        date.clear();
                        description.clear();
                    }
                    b"title" if in_item => field = Some(Field::Title),
                    b"link" if in_item => {
                        let mut href = String::new();
                        for attr in event.attributes().flatten() {
                            if attr.key.as_ref() == b"href" {
                                href = attr
                                    .unescape_value()
                                    .map(|value| value.to_string())
                                    .unwrap_or_default();
                            }
                        }
                        if !href.is_empty() {
                            link = href;
                        } else {
                            field = Some(Field::Link);
                        }
                    }
                    b"pubDate" | b"published" | b"updated" if in_item => {
                        field = Some(Field::Date)
                    }
                    b"description" | b"summary" | b"content" if in_item => {
                        field = Some(Field::Description)
                    }
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Empty(event)) => {
                if in_item && event.name().as_ref() == b"link" {
                    for attr in event.attributes().flatten() {
                        if attr.key.as_ref() == b"href" {
                            link = attr
                                .unescape_value()
                                .map(|value| value.to_string())
                                .unwrap_or_default();
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Text(text)) => {
                if let Some(target) = field {
                    let value = text.unescape().unwrap_or_default().to_string();
                    match target {
                        Field::Title => title.push_str(&value),
                        Field::Link => link.push_str(&value),
                        Field::Date => date.push_str(&value),
                        Field::Description => description.push_str(&value),
                    }
                }
            }
            Ok(quick_xml::events::Event::End(event)) => {
                if event.name().as_ref() == b"item" || event.name().as_ref() == b"entry" {
                    if !title.trim().is_empty() || !link.trim().is_empty() {
                        items.push(RssItem {
                            name: title.trim().to_string(),
                            date: date.trim().to_string(),
                            description: strip_html(description.trim()),
                            url: link.trim().to_string(),
                        });
                    }
                    in_item = false;
                    field = None;
                } else {
                    field = None;
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(error) => return Err(format!("RSS XML error: {error}")),
            _ => {}
        }
        buf.clear();
    }
    if items.is_empty() {
        return Err("No items found in feed".into());
    }
    Ok(items)
}

fn strip_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(character),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_an_rss_feed() {
        let xml = r#"<?xml version="1.0"?>
        <rss><channel><item>
          <title>Example release</title>
          <link>https://example.invalid/1.nzb</link>
          <pubDate>Tue, 03 Nov 2026 10:00:00 GMT</pubDate>
          <description>&lt;b&gt;A description&lt;/b&gt;</description>
        </item></channel></rss>"#;
        let items = parse_feed(xml).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Example release");
        assert_eq!(items[0].url, "https://example.invalid/1.nzb");
        assert_eq!(items[0].description, "A description");
    }

    #[test]
    fn parses_an_atom_feed() {
        let xml = r#"<feed><entry><title>Atom item</title><link href="https://example.invalid/a"/><updated>2026-11-03T10:00:00Z</updated></entry></feed>"#;
        let items = parse_feed(xml).unwrap();
        assert_eq!(items[0].name, "Atom item");
        assert_eq!(items[0].url, "https://example.invalid/a");
    }

    #[test]
    fn last_matching_filter_wins_and_p_resumes_paused() {
        let filters = parse_filters("*1080p* | AP\n*sample* | D\n*1080p*sample* | D");
        let accepted = last_matching_filter(&filters, "Movie.1080p.WEB-DL").unwrap();
        assert!(accepted.accept);
        assert!(accepted.pause);
        let denied = last_matching_filter(&filters, "Movie.1080p.sample").unwrap();
        assert!(!denied.accept);
        assert!(last_matching_filter(&filters, "Movie.720p").is_none());
    }
}
