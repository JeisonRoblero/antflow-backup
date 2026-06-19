// search.rs
// Provides web search capability using the DuckDuckGo Instant Answer API.
// When the instant answer is too short, a full page fetch is performed and
// the HTML is stripped to plain text before being returned to the AI.

use anyhow::{Context, Result};
use reqwest::Client;

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

// ---------------------------------------------------------------------------
// DuckDuckGo search
// ---------------------------------------------------------------------------

// Sends a query to the DuckDuckGo Instant Answer API and returns a plain-text
// summary. DDG returns JSON with an "AbstractText" field for definitions and
// an "Answer" field for direct answers.
//
// If the response is too short (< 80 chars), the first related topic URL is
// fetched and its page content is returned instead.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
}

pub async fn search(query: &str) -> Result<String> {
    let client = Client::builder()
        // DDG blocks requests without a browser-like user-agent header.
        .user_agent(BROWSER_UA)
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    // The DDG Instant Answer API endpoint. No API key required.
    // `format=json` returns machine-readable JSON.
    // `no_html=1` strips HTML tags from text fields.
    // `skip_disambig=1` skips disambiguation pages.
    let url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
        urlencoded(query)
    );

    let resp = client.get(&url).send().await
        .context("DuckDuckGo search request failed")?;

    let val: serde_json::Value = resp.json().await
        .context("Failed to parse DuckDuckGo response")?;

    // Try the abstract text first (definitions, Wikipedia summaries).
    let abstract_text = val["AbstractText"].as_str().unwrap_or("").trim().to_string();
    if abstract_text.len() >= 80 {
        return Ok(abstract_text);
    }

    // Try the direct answer field (unit conversions, math, etc.).
    let answer = val["Answer"].as_str().unwrap_or("").trim().to_string();
    if !answer.is_empty() {
        return Ok(answer);
    }

    // Fall back to fetching the first related topic URL.
    let first_url = val["RelatedTopics"][0]["FirstURL"]
        .as_str()
        .or_else(|| val["AbstractURL"].as_str())
        .unwrap_or("")
        .to_string();

    if !first_url.is_empty() {
        let page_text = fetch_page(&client, &first_url).await
            .unwrap_or_default();
        if !page_text.is_empty() {
            return Ok(page_text);
        }
    }

    // Instant Answer API returned nothing useful — fall back to DDG HTML search.
    if let Ok((html_summary, _, _)) = search_html(query).await {
        if !html_summary.starts_with("No clear result found") {
            return Ok(html_summary);
        }
    }

    // If nothing useful was found, return a clear message so the AI knows.
    Ok(format!("No clear result found for: {}. Try rephrasing the query.", query))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchImage {
    pub url: String,
    pub domain: String,
}

pub async fn search_detailed(query: &str) -> Result<(String, Vec<SearchResult>, Vec<SearchImage>)> {
    let client = Client::builder()
        .user_agent(BROWSER_UA)
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let url = format!(
        "https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1",
        urlencoded(query)
    );

    let resp = client.get(&url).send().await
        .context("DuckDuckGo search request failed")?;

    let val: serde_json::Value = resp.json().await
        .context("Failed to parse DuckDuckGo response")?;

    let mut images = Vec::new();
    let mut seen_images = std::collections::HashSet::new();

    // Try primary image first
    let mut image_url = val["Image"].as_str().unwrap_or("").to_string();
    if !image_url.is_empty() {
        if !image_url.starts_with("http") {
            image_url = format!("https://duckduckgo.com{}", image_url);
        }
        let domain = extract_domain(&image_url);
        seen_images.insert(image_url.clone());
        images.push(SearchImage {
            url: image_url,
            domain,
        });
    }

    // Try getting icons from RelatedTopics
    if let Some(topics) = val["RelatedTopics"].as_array() {
        for t in topics {
            if let Some(icon_url) = t["Icon"]["URL"].as_str() {
                if !icon_url.is_empty() {
                    let mut full_url = icon_url.to_string();
                    if !full_url.starts_with("http") {
                        full_url = format!("https://duckduckgo.com{}", full_url);
                    }
                    if seen_images.insert(full_url.clone()) {
                        let domain = extract_domain(&full_url);
                        images.push(SearchImage {
                            url: full_url,
                            domain,
                        });
                    }
                }
            } else if let Some(sub_topics) = t["Topics"].as_array() {
                for st in sub_topics {
                    if let Some(icon_url) = st["Icon"]["URL"].as_str() {
                        if !icon_url.is_empty() {
                            let mut full_url = icon_url.to_string();
                            if !full_url.starts_with("http") {
                                full_url = format!("https://duckduckgo.com{}", full_url);
                            }
                            if seen_images.insert(full_url.clone()) {
                                let domain = extract_domain(&full_url);
                                images.push(SearchImage {
                                    url: full_url,
                                    domain,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    let mut raw_topics = Vec::new();
    extract_topics(&val, &mut raw_topics);

    let mut results = Vec::new();
    let mut seen_urls = std::collections::HashSet::new();

    if let Some(abs_url) = val["AbstractURL"].as_str() {
        if !abs_url.is_empty() {
            let abs_title = val["Heading"].as_str().unwrap_or("Abstract Summary").to_string();
            let domain = extract_domain(abs_url);
            seen_urls.insert(abs_url.to_string());
            let abs_snippet = val["AbstractText"].as_str().unwrap_or("").to_string();
            results.push(SearchResult {
                title: abs_title,
                url: abs_url.to_string(),
                domain,
                snippet: abs_snippet,
            });
        }
    }

    for (u, text) in raw_topics {
        if seen_urls.insert(u.clone()) {
            let domain = extract_domain(&u);
            let mut title = text.clone();
            if let Some(pos) = title.find(" - ") {
                title = title[..pos].to_string();
            }
            let title_trimmed = title.trim();
            let final_title = if title_trimmed.is_empty() {
                format!("Related page on {}", domain)
            } else if title_trimmed.len() > 60 {
                title_trimmed.chars().take(60).collect::<String>() + "..."
            } else {
                title_trimmed.to_string()
            };
            let snippet = if let Some(pos) = text.find(" - ") {
                text[pos + 3..].trim().to_string()
            } else {
                text.clone()
            };
            results.push(SearchResult {
                title: final_title,
                url: u,
                domain,
                snippet,
            });
        }
    }

    if images.len() < 5 && !results.is_empty() {
        let mut fetch_tasks = Vec::new();
        for res in results.iter().take(3).cloned() {
            let client_clone = client.clone();
            fetch_tasks.push(tokio::spawn(async move {
                if let Ok(resp) = client_clone.get(&res.url).header("User-Agent", BROWSER_UA).send().await {
                    if let Ok(html) = resp.text().await {
                        return extract_images_from_html(&html, &res.url);
                    }
                }
                vec![]
            }));
        }
        for task in fetch_tasks {
            if let Ok(found_images) = task.await {
                for img in found_images {
                    if seen_images.insert(img.url.clone()) {
                        images.push(img);
                    }
                }
            }
        }
    }

    let abstract_text = val["AbstractText"].as_str().unwrap_or("").trim().to_string();
    if abstract_text.len() >= 80 {
        return Ok((abstract_text, results, images));
    }

    let answer = val["Answer"].as_str().unwrap_or("").trim().to_string();
    if !answer.is_empty() {
        return Ok((answer, results, images));
    }

    if let Some(first_res) = results.first() {
        let page_text = fetch_page(&client, &first_res.url).await
            .unwrap_or_default();
        if !page_text.is_empty() {
            return Ok((page_text, results, images));
        }
    }

    // Instant Answer API returned nothing useful — fall back to DDG HTML search.
    if let Ok((html_summary, html_results, html_images)) = search_html(query).await {
        if !html_summary.starts_with("No clear result found") {
            // Merge HTML results into existing results.
            let mut seen_urls: std::collections::HashSet<String> =
                results.iter().map(|r| r.url.clone()).collect();
            for r in html_results {
                if seen_urls.insert(r.url.clone()) {
                    results.push(r);
                }
            }
            let mut seen_img_urls: std::collections::HashSet<String> =
                images.iter().map(|i| i.url.clone()).collect();
            for img in html_images {
                if seen_img_urls.insert(img.url.clone()) {
                    images.push(img);
                }
            }
            return Ok((html_summary, results, images));
        }
    }

    Ok((
        format!("No clear result found for: {}. Try rephrasing the query.", query),
        results,
        images
    ))
}

// ---------------------------------------------------------------------------
// DuckDuckGo HTML search fallback
// ---------------------------------------------------------------------------

// Performs an actual web search by fetching the DuckDuckGo HTML-only search
// page and parsing the result divs. This is used as a fallback when the
// Instant Answer API returns nothing (which is most queries, since the API
// is a knowledge graph, not a search engine).
async fn search_html(query: &str) -> Result<(String, Vec<SearchResult>, Vec<SearchImage>)> {
    let client = Client::builder()
        .user_agent(BROWSER_UA)
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;

    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoded(query)
    );

    let resp = client.get(&url).send().await
        .context("DuckDuckGo HTML search request failed")?;

    let html = resp.text().await
        .context("Failed to read DuckDuckGo HTML search response")?;

    let mut results = Vec::new();
    let mut summary_parts = Vec::new();

    // Parse results from HTML. Each result lives in a div that contains:
    //   <a class="result__a" href="...">Title</a>
    //   <a class="result__snippet" ...>Snippet text</a>
    // We search for "result__a" anchors to find each result block.
    let mut search_pos = 0;
    let html_lower = html.to_lowercase();

    while results.len() < 8 {
        // Find the next result__a anchor.
        let anchor_marker = "class=\"result__a\"";
        let anchor_idx = match html_lower[search_pos..].find(anchor_marker) {
            Some(idx) => search_pos + idx,
            None => break,
        };

        // Extract the href from this anchor tag.
        // Walk backwards to find the opening '<a' of this tag.
        let tag_start = match html[..anchor_idx].rfind('<') {
            Some(s) => s,
            None => { search_pos = anchor_idx + anchor_marker.len(); continue; }
        };
        let tag_end = match html[tag_start..].find('>') {
            Some(e) => tag_start + e,
            None => { search_pos = anchor_idx + anchor_marker.len(); continue; }
        };
        let tag_content = &html[tag_start..tag_end];

        // Extract href value.
        let href = extract_attr_value(tag_content, "href");

        // Extract title text (content between <a ...> and </a>).
        let title_start = tag_end + 1;
        let title_end = match html_lower[title_start..].find("</a>") {
            Some(e) => title_start + e,
            None => { search_pos = tag_end; continue; }
        };
        let raw_title = strip_html(&html[title_start..title_end]).trim().to_string();

        // Move search_pos past this result.
        search_pos = title_end + 4;

        // Resolve the URL — DDG wraps links through a redirect like
        // //duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com&...
        let resolved_url = resolve_ddg_url(&href);
        if resolved_url.is_empty() || raw_title.is_empty() {
            continue;
        }

        // Extract snippet: look for result__snippet in the nearby HTML.
        let snippet_marker = "class=\"result__snippet";
        let snippet_text = if let Some(snip_idx) = html_lower[search_pos..].find(snippet_marker) {
            let snip_abs = search_pos + snip_idx;
            // Find the closing > of the snippet tag.
            if let Some(snip_tag_end) = html[snip_abs..].find('>') {
                let snip_content_start = snip_abs + snip_tag_end + 1;
                // Find the closing </a> or </span> or next tag boundary.
                let snip_content_end = html_lower[snip_content_start..]
                    .find("</a>")
                    .or_else(|| html_lower[snip_content_start..].find("</span>"))
                    .map(|e| snip_content_start + e)
                    .unwrap_or(snip_content_start);
                let raw_snippet = strip_html(&html[snip_content_start..snip_content_end]).trim().to_string();
                // Don't advance search_pos past snippet — it's within the same result block.
                raw_snippet
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let domain = extract_domain(&resolved_url);

        // Build summary line.
        if !snippet_text.is_empty() {
            summary_parts.push(format!("{}. {} — {}", results.len() + 1, raw_title, snippet_text));
        } else {
            summary_parts.push(format!("{}. {}", results.len() + 1, raw_title));
        }

        results.push(SearchResult {
            title: raw_title,
            url: resolved_url,
            domain,
            snippet: snippet_text,
        });
    }

    if results.is_empty() {
        return Ok((
            format!("No clear result found for: {}. Try rephrasing the query.", query),
            vec![],
            vec![],
        ));
    }

    let mut images = Vec::new();
    let mut seen_images = std::collections::HashSet::new();

    let mut fetch_tasks = Vec::new();
    for res in results.iter().take(4).cloned() {
        let client_clone = client.clone();
        fetch_tasks.push(tokio::spawn(async move {
            if let Ok(resp) = client_clone.get(&res.url).header("User-Agent", BROWSER_UA).send().await {
                if let Ok(html) = resp.text().await {
                    return extract_images_from_html(&html, &res.url);
                }
            }
            vec![]
        }));
    }
    for task in fetch_tasks {
        if let Ok(found_images) = task.await {
            for img in found_images {
                if seen_images.insert(img.url.clone()) {
                    images.push(img);
                }
            }
        }
    }

    let summary = format!(
        "DuckDuckGo search results for \"{}\":\n{}",
        query,
        summary_parts.join("\n")
    );

    Ok((summary, results, images))
}

// Extracts the value of an HTML attribute from a tag string.
// E.g. extract_attr_value(r#"<a href="https://x.com" class="foo""#, "href")
// returns "https://x.com".
fn extract_attr_value(tag: &str, attr: &str) -> String {
    let search = format!("{}=", attr);
    let pos = match tag.to_lowercase().find(&search) {
        Some(p) => p + search.len(),
        None => return String::new(),
    };
    let rest = &tag[pos..];
    let quote = rest.chars().next().unwrap_or('"');
    if quote == '"' || quote == '\'' {
        let mut val = String::new();
        for c in rest[1..].chars() {
            if c == quote { break; }
            val.push(c);
        }
        val
    } else {
        // Unquoted attribute — take until whitespace or >
        rest.split(|c: char| c.is_whitespace() || c == '>').next().unwrap_or("").to_string()
    }
}

// DuckDuckGo wraps external links through a redirect URL like:
//   //duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2F&rut=...
// This function extracts and decodes the real destination URL.
fn resolve_ddg_url(href: &str) -> String {
    // Direct external URL — return as-is.
    if href.starts_with("https://") || href.starts_with("http://") {
        // But skip duckduckgo.com internal pages (except redirect links).
        if href.contains("duckduckgo.com") && !href.contains("uddg=") {
            // Could be a redirect link — try to extract uddg param.
            if let Some(real) = extract_uddg_param(href) {
                return real;
            }
            return String::new();
        }
        if href.contains("uddg=") {
            if let Some(real) = extract_uddg_param(href) {
                return real;
            }
        }
        return href.to_string();
    }
    // Protocol-relative URL like //duckduckgo.com/l/?uddg=...
    if href.starts_with("//") {
        let full = format!("https:{}", href);
        if full.contains("uddg=") {
            if let Some(real) = extract_uddg_param(&full) {
                return real;
            }
        }
        return String::new(); // skip DDG internal links
    }
    // Relative URL like /l/?uddg=...
    if href.starts_with("/") {
        let full = format!("https://duckduckgo.com{}", href);
        if full.contains("uddg=") {
            if let Some(real) = extract_uddg_param(&full) {
                return real;
            }
        }
        return String::new();
    }
    String::new()
}

// Extracts and percent-decodes the `uddg` query parameter from a URL.
fn extract_uddg_param(url: &str) -> Option<String> {
    let query_start = url.find('?').map(|p| p + 1).unwrap_or(0);
    let query = &url[query_start..];
    for pair in query.split('&') {
        if let Some(val) = pair.strip_prefix("uddg=") {
            return Some(percent_decode(val));
        }
    }
    None
}

// Simple percent-decoding for URL values.
fn percent_decode(s: &str) -> String {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                &s[i + 1..i + 3], 16
            ) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn extract_topics(val: &serde_json::Value, list: &mut Vec<(String, String)>) {
    if let Some(topics) = val["RelatedTopics"].as_array() {
        for t in topics {
            if let Some(url) = t["FirstURL"].as_str() {
                let text = t["Text"].as_str().unwrap_or("").to_string();
                list.push((url.to_string(), text));
            } else if let Some(sub_topics) = t["Topics"].as_array() {
                for st in sub_topics {
                    if let Some(url) = st["FirstURL"].as_str() {
                        let text = st["Text"].as_str().unwrap_or("").to_string();
                        list.push((url.to_string(), text));
                    }
                }
            }
        }
    }
}

fn extract_domain(url: &str) -> String {
    let without_protocol = url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.");
    without_protocol.split('/').next().unwrap_or("").to_string()
}

// ---------------------------------------------------------------------------
// Page fetch + HTML strip
// ---------------------------------------------------------------------------

// Fetches a web page and returns the visible text, HTML tags removed.
// Keeps the first 3000 characters to stay within reasonable context size.
async fn fetch_page(client: &Client, url: &str) -> Result<String> {
    let resp = client
        .get(url)
        .header("User-Agent", BROWSER_UA)
        .send()
        .await
        .context("Page fetch failed")?;

    let html = resp.text().await?;
    let plain = strip_html(&html);

    // Trim to a reasonable length so it does not overflow the AI context window.
    let trimmed = plain.chars().take(3000).collect::<String>();
    Ok(trimmed)
}

// Strips HTML tags from a string using a simple state machine.
// This is intentionally minimal: no DOM parsing, no external crate.
// Edge cases like inline scripts are handled by skipping <script> blocks.
fn strip_html(html: &str) -> String {
    let mut output = String::with_capacity(html.len() / 2);
    let mut in_tag = false;
    let mut in_script = false;
    let mut buf = String::new();

    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if in_script {
            // Look for </script> to exit script block.
            if c == '<' && html[i..].to_lowercase().starts_with("</script") {
                in_script = false;
                // Skip past the closing tag.
                while i < chars.len() && chars[i] != '>' {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }

        if c == '<' {
            in_tag = true;
            buf.clear();
            i += 1;
            continue;
        }

        if c == '>' {
            // Check if we just closed a <script> tag.
            if buf.trim_start().to_lowercase().starts_with("script") {
                in_script = true;
            }
            in_tag = false;
            // Replace tag boundaries with a space to avoid words running together.
            output.push(' ');
            i += 1;
            continue;
        }

        if in_tag {
            buf.push(c);
        } else {
            output.push(c);
        }

        i += 1;
    }

    // Collapse multiple consecutive whitespace characters into one.
    let mut result = String::new();
    let mut prev_space = false;
    for c in output.chars() {
        if c.is_whitespace() {
            if !prev_space {
                result.push(' ');
            }
            prev_space = true;
        } else {
            result.push(c);
            prev_space = false;
        }
    }

    result.trim().to_string()
}

// ---------------------------------------------------------------------------
// URL encoding helper
// ---------------------------------------------------------------------------

// Percent-encodes a query string for safe use in a URL.
// Replaces spaces with + and encodes special characters.
fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push('+'),
            // These characters are safe in query strings.
            'A'..='Z' | 'a'..='z' | '0'..='9'
            | '-' | '_' | '.' | '~' => out.push(c),
            // Everything else is percent-encoded.
            other => {
                for byte in other.to_string().as_bytes() {
                    out.push('%');
                    out.push(HEX[(byte >> 4) as usize] as char);
                    out.push(HEX[(byte & 0xF) as usize] as char);
                }
            }
        }
    }
    out
}

const HEX: &[u8] = b"0123456789ABCDEF";

fn extract_images_from_html(html: &str, page_url: &str) -> Vec<SearchImage> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    
    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' && html[i..].to_lowercase().starts_with("<img") {
            let mut j = i + 4;
            let mut img_tag = String::new();
            while j < chars.len() && chars[j] != '>' {
                img_tag.push(chars[j]);
                j += 1;
            }
            if let Some(src_pos) = img_tag.find("src=") {
                let after_src = &img_tag[src_pos + 4..];
                let quote_char = after_src.chars().next().unwrap_or('"');
                if quote_char == '"' || quote_char == '\'' {
                    let mut src_val = String::new();
                    for c in after_src[1..].chars() {
                        if c == quote_char {
                            break;
                        }
                        src_val.push(c);
                    }
                    if !src_val.is_empty() && (src_val.starts_with("http") || src_val.starts_with("//") || src_val.starts_with("/")) {
                        let mut full_img_url = src_val;
                        if full_img_url.starts_with("//") {
                            full_img_url = format!("https:{}", full_img_url);
                        } else if full_img_url.starts_with("/") {
                            if let Some(pos) = page_url.find("://") {
                                let after_proto = &page_url[pos + 3..];
                                let domain_only = if let Some(slash_pos) = after_proto.find('/') {
                                    &page_url[..pos + 3 + slash_pos]
                                } else {
                                    page_url
                                };
                                full_img_url = format!("{}{}", domain_only, full_img_url);
                            }
                        }
                        let lower_url = full_img_url.to_lowercase();
                        if (lower_url.contains(".jpg") || lower_url.contains(".jpeg") || lower_url.contains(".png") || lower_url.contains(".webp"))
                           && !lower_url.contains("pixel") && !lower_url.contains("tracker") && !lower_url.contains("analytics") && !lower_url.contains("logo") && !lower_url.contains("icon") {
                            let domain = extract_domain(&full_img_url);
                            if seen.insert(full_img_url.clone()) {
                                found.push(SearchImage {
                                    url: full_img_url,
                                    domain,
                                });
                            }
                        }
                    }
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    found
}
