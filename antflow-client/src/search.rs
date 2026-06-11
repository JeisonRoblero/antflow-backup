// search.rs
// Provides web search capability using the DuckDuckGo Instant Answer API.
// When the instant answer is too short, a full page fetch is performed and
// the HTML is stripped to plain text before being returned to the AI.

use anyhow::{Context, Result};
use reqwest::Client;

// ---------------------------------------------------------------------------
// DuckDuckGo search
// ---------------------------------------------------------------------------

// Sends a query to the DuckDuckGo Instant Answer API and returns a plain-text
// summary. DDG returns JSON with an "AbstractText" field for definitions and
// an "Answer" field for direct answers.
//
// If the response is too short (< 80 chars), the first related topic URL is
// fetched and its page content is returned instead.
pub async fn search(query: &str) -> Result<String> {
    let client = Client::builder()
        // DDG blocks requests without a user-agent header.
        .user_agent("antflow/0.1")
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

    // If nothing useful was found, return a clear message so the AI knows.
    Ok(format!("No clear result found for: {}. Try rephrasing the query.", query))
}

// ---------------------------------------------------------------------------
// Page fetch + HTML strip
// ---------------------------------------------------------------------------

// Fetches a web page and returns the visible text, HTML tags removed.
// Keeps the first 3000 characters to stay within reasonable context size.
async fn fetch_page(client: &Client, url: &str) -> Result<String> {
    let resp = client
        .get(url)
        .header("User-Agent", "antflow/0.1")
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
