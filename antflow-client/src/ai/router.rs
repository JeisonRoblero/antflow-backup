use rust_embed::RustEmbed;
use serde_json::Value;
use std::sync::OnceLock;
use crate::ai::embeddings::LocalEmbedder;

#[derive(RustEmbed)]
#[folder = "internal_skills/"]
struct InternalSkillsAsset;

#[derive(RustEmbed)]
#[folder = "tools/"]
struct ToolsAsset;

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub id: String,
    pub friendly_name: String,
    pub description: String,
    pub full_content: String,
    pub embedding: Vec<f32>,
    pub file_path: String,
}

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub id: String,
    pub description: String,
    pub schema_json: Value,
    pub embedding: Vec<f32>,
}

pub struct RouterCache {
    pub internal_skills: Vec<SkillInfo>,
    pub tools: Vec<ToolInfo>,
}

static ROUTER_CACHE: OnceLock<RouterCache> = OnceLock::new();
static EMBEDDER: OnceLock<LocalEmbedder> = OnceLock::new();

pub fn get_embedder() -> Result<&'static LocalEmbedder, String> {
    if let Some(emb) = EMBEDDER.get() {
        return Ok(emb);
    }
    match LocalEmbedder::new() {
        Ok(emb) => {
            let _ = EMBEDDER.set(emb);
            Ok(EMBEDDER.get().unwrap())
        }
        Err(e) => Err(e.to_string()),
    }
}

pub fn get_router_cache() -> &'static RouterCache {
    ROUTER_CACHE.get_or_init(|| {
        match init_router_cache() {
            Ok(cache) => cache,
            Err(e) => {
                eprintln!("[ROUTER ERROR] Failed to initialize router cache: {:?}", e);
                RouterCache {
                    internal_skills: Vec::new(),
                    tools: Vec::new(),
                }
            }
        }
    })
}

fn parse_skill_md(content: &str) -> Option<(String, String, String)> {
    if !content.starts_with("---") {
        return None;
    }
    let after_dash = &content[3..];
    let end_dash = after_dash.find("\n---")
        .or_else(|| after_dash.find("\r\n---"))?;
    let yaml = &after_dash[..end_dash];
    
    let mut id = String::new();
    let mut friendly_name = String::new();
    let mut description = String::new();
    
    for line in yaml.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim();
            let val = line[pos + 1..].trim().trim_matches('"').trim_matches('\'').trim().to_string();
            match key {
                "id" => id = val,
                "friendly_name" => friendly_name = val,
                "description" => description = val,
                _ => {}
            }
        }
    }
    
    if id.is_empty() || description.is_empty() {
        return None;
    }
    Some((id, friendly_name, description))
}

fn init_router_cache() -> anyhow::Result<RouterCache> {
    let embedder = get_embedder().map_err(|e| anyhow::anyhow!("Failed to initialize embedder: {}", e))?;
    let mut internal_skills = Vec::new();
    let mut tools = Vec::new();

    // Load internal skills
    for file in InternalSkillsAsset::iter() {
        if let Some(file_data) = InternalSkillsAsset::get(&file) {
            if let Ok(content_str) = std::str::from_utf8(&file_data.data) {
                if let Some((id, friendly_name, description)) = parse_skill_md(content_str) {
                    if let Ok(emb) = embedder.encode(&description) {
                        internal_skills.push(SkillInfo {
                            id,
                            friendly_name,
                            description,
                            full_content: content_str.to_string(),
                            embedding: emb,
                            file_path: file.to_string(),
                        });
                    }
                }
            }
        }
    }

    // Load tools
    for file in ToolsAsset::iter() {
        if let Some(file_data) = ToolsAsset::get(&file) {
            if let Ok(schema_json) = serde_json::from_slice::<Value>(&file_data.data) {
                if let Some(desc) = schema_json.get("description").and_then(|v| v.as_str()) {
                    let name = schema_json.get("name").and_then(|v| v.as_str()).unwrap_or(file.as_ref()).to_string();
                    if let Ok(emb) = embedder.encode(desc) {
                        tools.push(ToolInfo {
                            id: name,
                            description: desc.to_string(),
                            schema_json,
                            embedding: emb,
                        });
                    }
                }
            }
        }
    }

    Ok(RouterCache { internal_skills, tools })
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot_product = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for (val_a, val_b) in a.iter().zip(b.iter()) {
        dot_product += val_a * val_b;
        norm_a += val_a * val_a;
        norm_b += val_b * val_b;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot_product / (norm_a.sqrt() * norm_b.sqrt())
}

/// Returns true if the prompt is a conversational / meta / UI-navigation request
/// that should NOT be enriched with task-specific tokens.
fn is_meta_or_chat_query(lower: &str) -> bool {
    let trimmed = lower.trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c == '¡' || c == '¿' || c.is_whitespace());

    // Exact short greetings / meta phrases
    let exact = [
        "hello", "hi", "hey", "hola", "yo", "sup", "bye", "goodbye",
        "thanks", "thank you", "gracias", "buenos dias", "buenas noches", "buenas tardes",
    ];
    if exact.iter().any(|p| trimmed == *p) {
        return true;
    }

    // Substring patterns that indicate UI / meta requests, NOT real tasks
    let meta_phrases = [
        "show me options", "show options", "give me options", "give me choices",
        "what can you do", "what are you", "who are you",
        "que puedes hacer", "qué puedes hacer", "que sabes hacer", "qué sabes hacer",
        "muestrame opciones", "muéstrame opciones", "dame opciones",
        "not a task", "no es una tarea", "just chat", "solo responde",
        "don't search", "dont search", "no busques", "no searches",
    ];
    if meta_phrases.iter().any(|p| trimmed.contains(p)) {
        return true;
    }

    false
}

fn enrich_routing_query(prompt: &str) -> String {
    let lower = prompt.to_lowercase();

    // Early exit: conversational / meta queries must NOT be enriched
    if is_meta_or_chat_query(&lower) {
        return prompt.to_string();
    }

    let mut query = prompt.to_string();
    
    if lower.contains("escribe") || lower.contains("teclea") || lower.contains("digita") || lower.contains("escribir")
        || lower.contains("write") || lower.contains("type") || lower.contains("keyboard") || lower.contains("text") {
        query.push_str(" write type text keyboard type_text");
    }
    if lower.contains("pantalla") || lower.contains("captura") || lower.contains("pantallazo")
        || lower.contains("screenshot") || lower.contains("screen") || lower.contains("capture") || lower.contains("image") {
        query.push_str(" screenshot image screen capture");
    }
    if lower.contains("clic") || lower.contains("click") || lower.contains("presiona") || lower.contains("presionar")
        || lower.contains("press") {
        query.push_str(" mouse click press mouse_click");
    }
    if lower.contains("arrastra") || lower.contains("desplaza") || lower.contains("arrastrar")
        || lower.contains("drag") || lower.contains("swipe") || lower.contains("drop") {
        query.push_str(" mouse drag drop swipe mouse_drag");
    }
    if lower.contains("espera") || lower.contains("demora") || lower.contains("esperar")
        || lower.contains("wait") || lower.contains("sleep") || lower.contains("delay") {
        query.push_str(" wait delay sleep");
    }
    if lower.contains("ejecuta") || lower.contains("corre") || lower.contains("inicia") || lower.contains("ejecutar")
        || lower.contains("run") || lower.contains("execute") || lower.contains("start") {
        query.push_str(" run command execute script run_command run_script");
    }
    if lower.contains("abre") || lower.contains("abrir") || lower.contains("lanza") || lower.contains("lanzar")
        || lower.contains("open") || lower.contains("launch") {
        query.push_str(" open launch start winword excel notepad open_url");
    }
    // "find" removed — too broad (matches "find out", "find me options", etc.)
    // Only trigger on explicit search verbs
    if lower.contains("busca") || lower.contains("buscar") || lower.contains("googlea")
        || lower.contains("search") || lower.contains("google") || lower.contains("lookup") || lower.contains("look up") {
        query.push_str(" web search find query google web_search");
    }
    if lower.contains("portapapeles") || lower.contains("copia") || lower.contains("pega") || lower.contains("copiar") || lower.contains("pegar")
        || lower.contains("clipboard") || lower.contains("copy") || lower.contains("paste") {
        query.push_str(" clipboard copy paste set get clipboard_set");
    }
    if lower.contains("word") || lower.contains("documento") || lower.contains("escribe")
        || lower.contains("document") || lower.contains("doc") {
        query.push_str(" word winword document type run_script run_command");
    }
    if lower.contains("draw") || lower.contains("paint") || lower.contains("dibujar") || lower.contains("pintar")
        || lower.contains("sketch") || lower.contains("doodle") || lower.contains("mspaint") {
        query.push_str(" mouse_click mouse_drag mouse_move screenshot mspaint paint draw");
    }
    if lower.contains("map") || lower.contains("mapa") || lower.contains("location") || lower.contains("ubicacion")
        || lower.contains("route") || lower.contains("ruta") || lower.contains("pin") || lower.contains("place") || lower.contains("lugar") {
        query.push_str(" map location route pin place show display render draw");
    }
    
    query
}

macro_rules! trace_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            let log_path = std::env::temp_dir().join("antflow_debug.log");
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true).append(true)
                .open(&log_path)
            {
                use std::io::Write;
                let _ = writeln!(f, $($arg)*);
            }
        }
    };
}

pub fn route_prompt(prompt: &str, threshold: f32) -> (Vec<(SkillInfo, f32)>, Vec<(ToolInfo, f32)>) {
    let cache = get_router_cache();
    let embedder = match get_embedder() {
        Ok(e) => e,
        Err(e) => {
            let err_msg = format!("Failed to instantiate LocalEmbedder: {:?}", e);
            eprintln!("[ROUTER ERROR] {}", err_msg);
            trace_log!("[ROUTER ERROR] {}", err_msg);
            return (Vec::new(), Vec::new());
        }
    };

    let enriched = enrich_routing_query(prompt);
    trace_log!("[ROUTER] Enriched query: '{}'", enriched);

    let prompt_emb = match embedder.encode(&enriched) {
        Ok(emb) => emb,
        Err(e) => {
            let err_msg = format!("Failed to encode prompt: {:?}", e);
            eprintln!("[ROUTER ERROR] {}", err_msg);
            trace_log!("[ROUTER ERROR] {}", err_msg);
            return (Vec::new(), Vec::new());
        }
    };

    // Score internal skills
    let mut matched_skills = Vec::new();
    for skill in &cache.internal_skills {
        let score = cosine_similarity(&prompt_emb, &skill.embedding);
        if score >= threshold {
            matched_skills.push((skill.clone(), score));
        }
    }
    // Sort skills by score descending
    matched_skills.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Score tools
    let mut matched_tools = Vec::new();
    for tool in &cache.tools {
        if tool.id == "task_complete" || tool.id == "screenshot" || tool.id == "run_command" || tool.id == "run_script" {
            continue; // Exclude execution and utility tools from routing entry points
        }
        let score = cosine_similarity(&prompt_emb, &tool.embedding);
        if score >= threshold {
            matched_tools.push((tool.clone(), score));
        }
    }

    // Sort tools by score descending
    matched_tools.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Log only final matched results (not every individual cosine similarity)
    trace_log!("[ROUTER] Matched internal skills: {:?}", matched_skills.iter().map(|(s, score)| format!("{} ({:.4})", s.id, score)).collect::<Vec<_>>());
    trace_log!("[ROUTER] Matched tools: {:?}", matched_tools.iter().map(|(t, score)| format!("{} ({:.4})", t.id, score)).collect::<Vec<_>>());

    (matched_skills, matched_tools)
}
