// Shared CHAT vs TASK intent detection used by the gate and semantic router.

/// Returns true when the prompt is conversational / meta / UI-navigation and should not
/// enter desktop automation task mode.
pub fn is_meta_or_chat_query(lower: &str) -> bool {
    let trimmed = lower
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c == '¡' || c == '¿' || c.is_whitespace());

    let exact = [
        "hello",
        "hi",
        "hey",
        "hola",
        "yo",
        "sup",
        "bye",
        "goodbye",
        "thanks",
        "thank you",
        "gracias",
        "buenos dias",
        "buenas noches",
        "buenas tardes",
        "ay",
        "ok",
        "okay",
    ];
    if exact.iter().any(|p| trimmed == *p) {
        return true;
    }

    let meta_phrases = [
        "show me options",
        "show options",
        "show me some options",
        "give me options",
        "give me the options",
        "give me choices",
        "give me some choices",
        "what can you do",
        "what are you",
        "who are you",
        "que puedes hacer",
        "qué puedes hacer",
        "que sabes hacer",
        "qué sabes hacer",
        "muestrame opciones",
        "muéstrame opciones",
        "dame opciones",
        "not a task",
        "no es una tarea",
        "just chat",
        "solo responde",
        "im chatting",
        "i'm chatting",
        "im just asking",
        "i'm just asking",
        "don't search",
        "dont search",
        "no busques",
        "no searches",
        "select one",
        "so i can select",
        "here in the text",
        "in the chat",
        "por el chat",
        "en el chat",
    ];
    meta_phrases.iter().any(|p| trimmed.contains(p))
}

/// User asking what media types the agent can create (should stay in chat).
pub fn is_media_capability_list_query(lower: &str) -> bool {
    let trimmed = lower.trim();
    (trimmed.contains("list") && (trimmed.contains("media") || trimmed.contains("medias")))
        || trimmed.contains("what kind of media")
        || trimmed.contains("what kinds of media")
        || trimmed.contains("what types of media")
        || trimmed.contains("types of media")
        || trimmed.contains("medias you can")
        || trimmed.contains("media you can")
        || trimmed.contains("what can you create")
        || trimmed.contains("what can you generate")
        || trimmed.contains("what medias")
        || trimmed.contains("que medios")
        || trimmed.contains("qué medios")
        || trimmed.contains("que tipos de media")
        || trimmed.contains("qué tipos de media")
}

/// Questions about what the agent can do (not requests to perform automation).
pub fn is_capability_question(lower: &str) -> bool {
    let trimmed = lower.trim();
    let asks_capability = trimmed.starts_with("can you ")
        || trimmed.starts_with("could you ")
        || trimmed.starts_with("do you ")
        || trimmed.starts_with("are you able")
        || trimmed.starts_with("puedes ")
        || trimmed.starts_with("podrias ")
        || trimmed.starts_with("podrías ")
        || trimmed.contains("what can you do")
        || trimmed.contains("what are your capabilities");

    if !asks_capability {
        return false;
    }

    let automation_verbs = [
        "click",
        "clic",
        "screenshot",
        "captura",
        "drag",
        "arrastra",
        "open word and",
        "open excel and",
        "type this into",
        "escribe en",
    ];
    !automation_verbs.iter().any(|v| trimmed.contains(v))
}

pub fn is_anti_task_correction(lower: &str) -> bool {
    let phrases = [
        "no action",
        "just chat",
        "just answer",
        "just tell me",
        "just respond",
        "solo responde",
        "solo dime",
        "solo contesta",
        "ninguna accion",
        "sin accion",
        "no ejecutes",
        "no hagas nada",
        "solo por aqui",
        "en el chat",
        "por el chat",
        "don't do anything",
        "no need to do",
        "no action required",
        "no action needed",
        "just here",
        "chat only",
        "conversational",
        "no task",
        "not a task",
        "is not a task",
        "isn't a task",
        "im chatting",
        "i'm chatting",
        "damelo aqui",
        "responde aqui",
        "dime aqui",
    ];
    phrases.iter().any(|p| lower.contains(p))
}

/// Parse the gate classifier model output. Defaults to CHAT when ambiguous.
pub fn gate_classifier_says_task(response: &str) -> bool {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return false;
    }

    let upper = trimmed.to_uppercase();
    if upper.contains("NOT A TASK") || upper.contains("NOT TASK") || upper.contains("IS CHAT") {
        return false;
    }

    let first = upper
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphanumeric());

    match first {
        "CHAT" => false,
        "TASK" => true,
        _ => {
            if upper.starts_with("CHAT") {
                false
            } else if upper.starts_with("TASK") {
                true
            } else if upper.contains("CHAT") {
                false
            } else {
                false
            }
        }
    }
}

pub fn should_force_chat_mode(prompt: &str, is_obvious_task: bool, cfg_mode: &str) -> bool {
    if cfg_mode == "chat" {
        return true;
    }
    if cfg_mode == "task" {
        return false;
    }
    if is_obvious_task {
        return false;
    }

    let lower = prompt.to_lowercase();
    if is_meta_or_chat_query(&lower)
        || is_capability_question(&lower)
        || is_media_capability_list_query(&lower)
        || is_anti_task_correction(&lower)
    {
        return true;
    }

    false
}

pub fn is_task_affirmation(text: &str) -> bool {
    let lower = text.to_lowercase();
    let trimmed = lower
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c == '¡' || c == '¿');
    [
        "ok",
        "okay",
        "yes",
        "yep",
        "sure",
        "do it",
        "do it now",
        "okay do it",
        "ok do it",
        "go ahead",
        "go for it",
        "please do",
        "hazlo",
        "hazlo ya",
        "vale",
        "si",
        "sí",
        "adelante",
        "proceed",
        "continue",
        "start",
        "lets go",
        "let's go",
        "now",
        "ya",
        "ahora",
    ]
    .iter()
    .any(|a| trimmed == *a || trimmed.starts_with(&format!("{a} ")))
}

pub fn is_general_media_capability_question(lower: &str) -> bool {
    let trimmed = lower.trim().trim_matches(|c: char| c.is_ascii_punctuation() || c == '¡' || c == '¿' || c.is_whitespace());
    
    if is_media_capability_list_query(trimmed) {
        return true;
    }
    
    let general_questions = [
        "can you generate images",
        "can you create images",
        "can you draw",
        "can you generate video",
        "can you generate music",
        "do you generate images",
        "do you create images",
        "are you able to generate images",
        "are you able to create images",
        "are you able to draw",
        "puedes generar imagenes",
        "puedes generar imágenes",
        "puedes crear imagenes",
        "puedes crear imágenes",
        "puedes dibujar",
        "sabes dibujar",
        "sabes generar imagenes",
        "sabes generar imágenes",
        "generas imagenes",
        "generas imágenes",
    ];
    
    let check_str = if trimmed.starts_with("hola") {
        trimmed["hola".len()..].trim().trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
    } else if trimmed.starts_with("hello") {
        trimmed["hello".len()..].trim().trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
    } else if trimmed.starts_with("hi") {
        trimmed["hi".len()..].trim().trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
    } else {
        trimmed
    };
    
    general_questions.iter().any(|&q| {
        trimmed == q 
            || check_str == q 
            || trimmed.contains(&format!(" {q}")) 
            || check_str.contains(&format!(" {q}"))
    })
}

pub fn is_explicit_image_request(text: &str) -> bool {
    let lower = text.to_lowercase();
    if is_general_media_capability_question(&lower) {
        return false;
    }
    lower.contains("generate an image")
        || lower.contains("generate a image")
        || lower.contains("generate image")
        || lower.contains("create an image")
        || lower.contains("create a image")
        || lower.contains("generar imagen")
        || lower.contains("generar una imagen")
        || lower.contains("crear imagen")
        || lower.contains("crear una imagen")
        || lower.contains("draw me an")
        || lower.contains("draw a ")
        || lower.contains("picture of")
        || lower.contains("image of")
        || lower.contains("dibujame")
        || lower.contains("dibújame")
        || lower.contains("imagen de")
}


pub fn has_pending_image_generation(messages: &[crate::ai::ChatMessage]) -> bool {
    messages.iter().any(|m| {
        m.role == "user"
            && m
                .content
                .as_str()
                .map(is_explicit_image_request)
                .unwrap_or(false)
    })
}

/// Strip injected chat-mode system notes and other non-user content from message text.
pub fn sanitize_user_message_text(text: &str) -> String {
    let mut s = text.trim().to_string();
    if let Some(idx) = s.find("(System note:") {
        s = s[..idx].trim().to_string();
    }
    if let Some(idx) = s.find("\n\n(System note:") {
        s = s[..idx].trim().to_string();
    }
    if s.starts_with("User query:") {
        s = s["User query:".len()..].trim().to_string();
        if let Some(note_idx) = s.find("(System note:") {
            s = s[..note_idx].trim().to_string();
        }
    }
    s.trim().to_string()
}

/// Build a full image prompt from the original request plus clarification answers.
pub fn compose_media_prompt_from_history(
    prompt: &str,
    messages: &[crate::ai::ChatMessage],
) -> Option<String> {
    let mut base: Option<String> = None;
    let mut details: Vec<String> = Vec::new();

    let skip = |t: &str| -> bool {
        let tl = t.to_lowercase();
        tl.contains("give me the options")
            || tl.contains("list all")
            || tl.contains("show me options")
            || tl.contains("show options")
    };

    for msg in messages.iter() {
        if msg.role != "user" {
            continue;
        }
        let Some(text) = msg.content.as_str() else {
            continue;
        };
        let t = sanitize_user_message_text(text);
        if t.is_empty() || is_task_affirmation(&t) || skip(&t) {
            continue;
        }
        if is_explicit_image_request(&t) {
            base = Some(t);
        } else if is_general_media_capability_question(&t) || is_media_capability_list_query(&t) {
            continue;
        } else if t.len() >= 3 {
            details.push(t);
        }
    }

    let clean_prompt = sanitize_user_message_text(prompt);

    if is_task_affirmation(&clean_prompt) || has_pending_image_generation(messages) {
        if let Some(b) = base {
            let mut parts = vec![b];
            parts.extend(details);
            let composed = parts.join(", ");
            return Some(composed);
        }
    }

    if is_explicit_image_request(&clean_prompt) {
        let mut parts = vec![clean_prompt];
        parts.extend(details);
        return Some(parts.join(", "));
    }

    None
}

/// Resolve short affirmations ("okay do it") from recent user messages.
pub fn resolve_task_prompt_from_history(prompt: &str, messages: &[crate::ai::ChatMessage]) -> String {
    if let Some(composed) = compose_media_prompt_from_history(prompt, messages) {
        crate::dbg_log!(
            "[run_task] Composed media prompt from history: '{}'",
            composed
        );
        // #region agent log
        crate::debug_log::session_log(
            "intent.rs:resolve",
            "composed media prompt from history",
            serde_json::json!({ "prompt": prompt, "composed": composed }),
            "H2",
            "pre-fix",
        );
        // #endregion
        return composed;
    }

    if !is_task_affirmation(prompt) {
        return prompt.to_string();
    }

    let skip_phrases = [
        "generate an image",
        "generate audio",
        "give me the options",
        "what would you like",
        "list all",
        "show me options",
        "show options",
        "okay do it",
        "ok do it",
    ];

    for msg in messages.iter().rev() {
        if msg.role != "user" {
            continue;
        }
        if let Some(text) = msg.content.as_str() {
            let t = text.trim();
            if t.is_empty() || is_task_affirmation(t) {
                continue;
            }
            let tl = t.to_lowercase();
            if skip_phrases.iter().any(|p| tl.contains(p)) {
                continue;
            }
            if t.len() >= 3 {
                crate::dbg_log!(
                    "[run_task] Resolved affirmation '{}' → context: '{}'",
                    prompt,
                    t
                );
                return t.to_string();
            }
        }
    }
    prompt.to_string()
}

pub fn is_media_generation_task(prompt: &str) -> bool {
    if is_task_affirmation(prompt) {
        return false;
    }
    let lower = prompt.to_lowercase();
    if is_general_media_capability_question(&lower) {
        return false;
    }
    [
        "generate image",
        "generate an image",
        "create image",
        "draw ",
        "picture of",
        "image of",
        "generar imagen",
        "crear imagen",
        "dibujar",
        "imagen de",
        "generate speech",
        "generate audio",
        "text to speech",
        "read aloud",
        "generate video",
        "generate music",
    ]
    .iter()
    .any(|k| lower.contains(k))
        || is_explicit_image_request(prompt)
        || is_likely_image_prompt(prompt)
}

pub fn is_likely_image_prompt(prompt: &str) -> bool {
    if is_task_affirmation(prompt) {
        return false;
    }
    let lower = prompt.to_lowercase();
    if is_general_media_capability_question(&lower) {
        return false;
    }
    if is_explicit_image_request(prompt) {
        return true;
    }
    if lower.contains('?') {
        return false;
    }
    let automation_verbs = [
        "click", "open ", "screenshot", "run ", "execute", "type ", "write ",
        "busca", "search", "list all", "show me",
    ];
    if automation_verbs.iter().any(|v| lower.contains(v)) {
        return false;
    }
    let words: Vec<&str> = lower.split_whitespace().collect();
    (3..=12).contains(&words.len())
}


pub fn is_non_visual_task(prompt: &str) -> bool {
    if is_media_generation_task(prompt) {
        return true;
    }
    let lower = prompt.to_lowercase();
    [
        "voice",
        "speech",
        "speak",
        "tts",
        "text-to-speech",
        "text to speech",
        "generate a voice",
        "generar voz",
        "generar una voz",
        "audio narration",
        "read aloud",
        "read this aloud",
        "narrat",
        "voz",
        "hablar",
        "synthesize",
    ]
    .iter()
    .any(|k| lower.contains(k))
}
