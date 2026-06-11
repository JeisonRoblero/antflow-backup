// json_utils.rs
// Helpers for extracting and sanitising JSON from AI responses.

pub fn strip_fences(raw: &str) -> String {
    let s = raw.trim();
    if let Some(inner) = s.strip_prefix("```json") {
        return inner.trim_end_matches("```").trim().to_string();
    }
    if let Some(inner) = s.strip_prefix("```") {
        return inner.trim_end_matches("```").trim().to_string();
    }
    s.to_string()
}

pub fn extract_best_json(text: &str) -> Option<String> {
    let text = text.trim();
    if serde_json::from_str::<serde_json::Value>(text).is_ok() {
        return Some(text.to_string());
    }
    if let Some(candidate) = find_outermost_json(text) {
        if serde_json::from_str::<serde_json::Value>(&candidate).is_ok() {
            return Some(candidate);
        }
    }
    if let Some(start) = text.find("```json") {
        let inner = &text[start + 7..];
        if let Some(end) = inner.find("```") {
            let block = inner[..end].trim();
            if serde_json::from_str::<serde_json::Value>(block).is_ok() {
                return Some(block.to_string());
            }
        }
    }
    None
}

pub fn extract_json(text: &str) -> String {
    extract_best_json(text).unwrap_or_else(|| text.to_string())
}

fn find_outermost_json(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;
    let mut end = None;
    for (i, &b) in bytes[start..].iter().enumerate() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match b {
            b'\\' if in_string => escape_next = true,
            b'"' => in_string = !in_string,
            b'{' if !in_string => depth += 1,
            b'}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + i);
                    break;
                }
            }
            _ => {}
        }
    }
    end.map(|e| text[start..=e].to_string())
}

pub fn clean_if_json(text: &str) -> String {
    let trimmed = text.trim();
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(msg) = val["message"].as_str() {
            return msg.to_string();
        }
        if let Some(summary) = val["summary"].as_str() {
            return summary.to_string();
        }
    }
    let mut raw = trimmed.to_string();
    if raw.starts_with("```") {
        let lines: Vec<&str> = raw.lines().collect();
        if lines.len() >= 2 {
            let start_idx = if lines[0].starts_with("```json") || lines[0] == "```" { 1 } else { 0 };
            let end_idx = if lines[lines.len() - 1] == "```" { lines.len() - 1 } else { lines.len() };
            if start_idx < end_idx {
                let inner = lines[start_idx..end_idx].join("\n");
                let inner_trimmed = inner.trim();
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(inner_trimmed) {
                    if let Some(msg) = val["message"].as_str() {
                        return msg.to_string();
                    }
                    if let Some(summary) = val["summary"].as_str() {
                        return summary.to_string();
                    }
                }
            }
        }
    }
    text.to_string()
}

