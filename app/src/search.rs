//! Generic Newznab search and NZB download.

use chrono::{DateTime, Utc};
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub subject: String,
    pub size_mb: f64,
    pub age: f64,
    pub group: String,
    pub poster: String,
    pub url: String,
}

pub fn search(base_url: &str, api_key: &str, query: &str) -> Result<Vec<SearchHit>, String> {
    let base = base_url.trim();
    if base.is_empty() {
        return Err("Newznab URL is not configured".into());
    }
    let mut url = base.to_string();
    url.push(if url.contains('?') { '&' } else { '?' });
    url.push_str("t=search&extended=1&o=xml");
    if !api_key.trim().is_empty() {
        url.push_str("&apikey=");
        url.push_str(&percent_encode(api_key.trim()));
    }
    if !query.trim().is_empty() {
        url.push_str("&q=");
        url.push_str(&percent_encode(query.trim()));
    }
    let agent = agent()?;
    let response = agent.get(&url).call().map_err(safe_ureq_error)?;
    let mut body = String::new();
    response
        .into_reader()
        .read_to_string(&mut body)
        .map_err(|error| format!("read failed: {error}"))?;
    parse_results(&body)
}

pub fn fetch_nzb(url: &str) -> Result<Vec<u8>, String> {
    let agent = agent()?;
    let response = agent.get(url).call().map_err(safe_ureq_error)?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read failed: {error}"))?;
    Ok(bytes)
}

fn agent() -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|error| format!("TLS setup failed: {error}"))?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .timeout(Duration::from_secs(30))
        .user_agent("NewsQueue/1.0.1")
        .build())
}

fn safe_ureq_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => format!("HTTP {code}"),
        ureq::Error::Transport(transport) => format!("network error: {:?}", transport.kind()),
    }
}

fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn parse_results(xml: &str) -> Result<Vec<SearchHit>, String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut hits = Vec::new();
    let mut in_item = false;
    let mut field = "";
    let mut title = String::new();
    let mut link = String::new();
    let mut pub_date = String::new();
    let mut category = String::new();
    let mut author = String::new();
    let mut size_text = String::new();
    let mut attrs: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(event)) => {
                let qname = event.name();
                let name = local_name(qname.as_ref());
                match name {
                    b"item" => {
                        in_item = true;
                        title.clear();
                        link.clear();
                        pub_date.clear();
                        category.clear();
                        author.clear();
                        size_text.clear();
                        attrs.clear();
                    }
                    b"title" if in_item => field = "title",
                    b"link" if in_item => field = "link",
                    b"pubDate" if in_item => field = "pubDate",
                    b"category" if in_item => field = "category",
                    b"author" if in_item => field = "author",
                    b"size" if in_item => field = "size",
                    b"attr" if in_item => read_attr(&event, &mut attrs),
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::Empty(event)) => {
                if in_item && local_name(event.name().as_ref()) == b"attr" {
                    read_attr(&event, &mut attrs);
                }
            }
            Ok(quick_xml::events::Event::Text(text)) => {
                if in_item {
                    let value = text.unescape().unwrap_or_default();
                    match field {
                        "title" => title.push_str(&value),
                        "link" => link.push_str(&value),
                        "pubDate" => pub_date.push_str(&value),
                        "category" => category.push_str(&value),
                        "author" => author.push_str(&value),
                        "size" => size_text.push_str(&value),
                        _ => {}
                    }
                }
            }
            Ok(quick_xml::events::Event::End(event)) => {
                if local_name(event.name().as_ref()) == b"item" {
                    if !title.trim().is_empty() && !link.trim().is_empty() {
                        hits.push(make_hit(
                            &title,
                            &link,
                            &pub_date,
                            &category,
                            &author,
                            &size_text,
                            &attrs,
                        ));
                    }
                    in_item = false;
                }
                field = "";
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(error) => return Err(format!("Newznab XML error: {error}")),
            _ => {}
        }
        buf.clear();
    }
    if hits.is_empty() {
        return Err("No search results".into());
    }
    hits.truncate(100);
    Ok(hits)
}

fn read_attr(event: &quick_xml::events::BytesStart, attrs: &mut std::collections::HashMap<String, String>) {
    let mut name = String::new();
    let mut value = String::new();
    for attr in event.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
        let val = attr
            .unescape_value()
            .map(|value| value.to_string())
            .unwrap_or_default();
        if key == "name" {
            name = val.to_ascii_lowercase();
        } else if key == "value" {
            value = val;
        }
    }
    if !name.is_empty() {
        attrs.insert(name, value);
    }
}

fn make_hit(
    title: &str,
    link: &str,
    pub_date: &str,
    category: &str,
    author: &str,
    size_text: &str,
    attrs: &std::collections::HashMap<String, String>,
) -> SearchHit {
    let size_bytes = attrs
        .get("size")
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| size_text.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    let date = attrs
        .get("usenetdate")
        .map(String::as_str)
        .unwrap_or(pub_date);
    let age = parse_age(date);
    SearchHit {
        subject: title.trim().to_string(),
        size_mb: size_bytes / (1024.0 * 1024.0),
        age,
        group: attrs
            .get("group")
            .cloned()
            .unwrap_or_else(|| category.trim().to_string()),
        poster: attrs
            .get("poster")
            .cloned()
            .unwrap_or_else(|| author.trim().to_string()),
        url: link.trim().to_string(),
    }
}

fn parse_age(value: &str) -> f64 {
    let parsed = DateTime::parse_from_rfc2822(value)
        .map(|date| date.with_timezone(&Utc))
        .or_else(|_| DateTime::parse_from_rfc3339(value).map(|date| date.with_timezone(&Utc)));
    parsed
        .map(|date| {
            Utc::now()
                .signed_duration_since(date)
                .num_hours()
                .max(0) as f64
                / 24.0
        })
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_newznab_items_and_attributes() {
        let xml = r#"<rss xmlns:newznab="http://www.newznab.com/DTD/2010/feeds/attributes/"><channel>
        <item>
          <title>Example.Release.1080p</title>
          <link>https://index.invalid/get/1.nzb</link>
          <pubDate>Tue, 03 Nov 2026 10:00:00 GMT</pubDate>
          <category>Movies &gt; HD</category>
          <newznab:attr name="size" value="123456789"/>
          <newznab:attr name="poster" value="poster1"/>
          <newznab:attr name="group" value="alt.binaries.movies"/>
        </item>
        </channel></rss>"#;
        let hits = parse_results(xml).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].subject, "Example.Release.1080p");
        assert_eq!(hits[0].group, "alt.binaries.movies");
        assert_eq!(hits[0].poster, "poster1");
        assert!((hits[0].size_mb - 117.74).abs() < 0.1);
        assert!(!hits[0].url.is_empty());
    }
}
