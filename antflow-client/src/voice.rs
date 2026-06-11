#![cfg(any(feature = "gui", target_os = "android", target_os = "ios"))]

use std::sync::Arc;
use tokio::sync::Mutex;
use serde_json::json;
use tauri::Emitter;
use tauri::Manager;
use crate::gui::AppState;
use crate::ai::ChatMessage;
use crate::db;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VoiceState {
    Listening,
    Thinking,
    Speaking,
}

pub struct VoiceSession {
    pub state: VoiceState,
    pub audio_buffer: Vec<i16>,
    pub speech_samples: usize,
    pub silence_samples: usize,
    pub is_user_speaking: bool,
    pub app_handle: tauri::AppHandle,
    pub cancel_tx: Option<tokio::sync::watch::Sender<bool>>,
}

pub static ACTIVE_SESSION: std::sync::OnceLock<Mutex<Option<Arc<Mutex<VoiceSession>>>>> = std::sync::OnceLock::new();

pub fn active_session_state() -> &'static Mutex<Option<Arc<Mutex<VoiceSession>>>> {
    ACTIVE_SESSION.get_or_init(|| Mutex::new(None))
}

pub async fn start_session(app_handle: tauri::AppHandle) -> anyhow::Result<()> {
    let mut active = active_session_state().lock().await;
    // Always create a fresh session to ensure clean VAD and audio buffers
    let session = Arc::new(Mutex::new(VoiceSession {
        state: VoiceState::Listening,
        audio_buffer: Vec::new(),
        speech_samples: 0,
        silence_samples: 0,
        is_user_speaking: false,
        app_handle,
        cancel_tx: None,
    }));
    *active = Some(session);
    Ok(())
}

pub async fn stop_session() -> anyhow::Result<()> {
    let mut active = active_session_state().lock().await;
    if let Some(session_arc) = active.take() {
        let mut session = session_arc.lock().await;
        if let Some(tx) = session.cancel_tx.take() {
            let _ = tx.send(true);
        }
        let _ = session.app_handle.emit("stop-playback", serde_json::Value::Null);
    }
    Ok(())
}

pub async fn receive_chunk(base64_chunk: String) -> anyhow::Result<()> {
    let session_opt = {
        let active = active_session_state().lock().await;
        active.clone()
    };
    if let Some(session_arc) = session_opt {
        let mut session = session_arc.lock().await;
        session.process_chunk(base64_chunk).await?;
    }
    Ok(())
}

impl VoiceSession {
    pub async fn process_chunk(&mut self, base64_chunk: String) -> anyhow::Result<()> {
        let raw_bytes = match base64_decode(&base64_chunk) {
            Some(b) => b,
            None => {
                return Ok(());
            }
        };

        if raw_bytes.len() % 2 != 0 {
            return Ok(());
        }

        let mut samples = Vec::with_capacity(raw_bytes.len() / 2);
        for chunk in raw_bytes.chunks_exact(2) {
            let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
            samples.push(sample);
        }

        let n = samples.len();
        let mut sum = 0.0;
        for &sample in &samples {
            let f = sample as f32 / 32768.0;
            sum += f * f;
        }
        let rms = if n > 0 { (sum / n as f32).sqrt() * 100.0 } else { 0.0 };

        if self.state == VoiceState::Listening || self.state == VoiceState::Speaking {
            self.audio_buffer.extend_from_slice(&samples);
        }

        const SPEECH_THRESHOLD_RMS: f32 = 6.5;

        if rms > SPEECH_THRESHOLD_RMS {
            self.silence_samples = 0;
            self.speech_samples += n;

            if !self.is_user_speaking && self.speech_samples >= 1280 { // 80ms
                self.is_user_speaking = true;
            }

            if self.state == VoiceState::Speaking && self.is_user_speaking && self.speech_samples >= 2400 { // 150ms barge-in
                if let Some(tx) = self.cancel_tx.take() {
                    let _ = tx.send(true);
                }

                let _ = self.app_handle.emit("stop-playback", serde_json::Value::Null);

                self.audio_buffer.clear();
                self.speech_samples = 0;
                self.silence_samples = 0;
                self.is_user_speaking = false;
                self.state = VoiceState::Listening;

                let _ = self.app_handle.emit("voice-state", json!({ "state": "listening" }));
            }
        } else {
            if self.is_user_speaking {
                self.silence_samples += n;
                if self.silence_samples >= 12800 { // 800ms
                    self.is_user_speaking = false;
                    self.silence_samples = 0;
                    self.speech_samples = 0;

                    if self.state == VoiceState::Listening {
                        self.state = VoiceState::Thinking;
                        let _ = self.app_handle.emit("voice-state", json!({ "state": "thinking" }));

                        let app_handle = self.app_handle.clone();
                        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
                        self.cancel_tx = Some(cancel_tx);

                        let accumulated_samples = std::mem::take(&mut self.audio_buffer);
                         
                        let state = self.app_handle.state::<AppState>();
                        let session_id = state.session_id.lock().await.clone();

                        tokio::spawn(async move {
                            if let Err(e) = run_voice_pipeline(app_handle, session_id, accumulated_samples, cancel_rx).await {
                                                        eprintln!("Error running voice pipeline: {}", e);
                            }
                        });
                    }
                }
            } else {
                if self.state == VoiceState::Listening && self.audio_buffer.len() > 32000 {
                    let drain_len = self.audio_buffer.len() - 32000;
                    self.audio_buffer.drain(0..drain_len);
                }
                if self.speech_samples > n {
                    self.speech_samples -= n;
                } else {
                    self.speech_samples = 0;
                }
            }
        }

        Ok(())
    }
}

async fn run_voice_pipeline(
    app_handle: tauri::AppHandle,
    session_id: String,
    samples: Vec<i16>,
    mut cancel_rx: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    if samples.is_empty() {
        let _ = app_handle.emit("voice-state", json!({ "state": "listening" }));
        return Ok(());
    }

    let temp_dir = std::env::temp_dir();
    let wav_path = temp_dir.join("antflow_voice_input.wav");
    write_wav_file(&wav_path, &samples)?;

    let state = app_handle.state::<AppState>();
    let (provider_id, api_key, model_name, base_url) = {
        let cfg = state.cfg.lock().await;
        (
            cfg.ai.provider.clone(),
            cfg.ai.api_key.clone(),
            cfg.ai.model.clone(),
            cfg.ai.base_url.clone(),
        )
    };

    let mut stt_key = String::new();
    let mut stt_url = "https://api.openai.com/v1".to_string();
    let mut stt_provider = "openai".to_string();

    if provider_id == "openai" && !api_key.is_empty() {
        stt_key = api_key.clone();
    } else if provider_id == "groq" && !api_key.is_empty() {
        stt_key = api_key.clone();
        stt_url = "https://api.groq.com/openai/v1".to_string();
        stt_provider = "groq".to_string();
    } else {
        let keys = state.provider_keys_multi.lock().await;
        if let Some(val) = keys.get("openai").and_then(|v| v.first()) {
            if !val.is_empty() {
                stt_key = val.clone();
            }
        }
        if stt_key.is_empty() {
            if let Some(val) = keys.get("groq").and_then(|v| v.first()) {
                if !val.is_empty() {
                    stt_key = val.clone();
                    stt_url = "https://api.groq.com/openai/v1".to_string();
                    stt_provider = "groq".to_string();
                }
            }
        }
    }

    if stt_key.is_empty() {
        eprintln!("Voice: No API key found for Whisper transcription");
        let _ = app_handle.emit("voice-state", json!({ "state": "listening" }));
        reset_session_to_listening().await;
        return Ok(());
    }

    if *cancel_rx.borrow() { return Ok(()); }

    let client = reqwest::Client::new();
    let wav_bytes = std::fs::read(&wav_path)?;

    let url = format!("{}/audio/transcriptions", stt_url.trim_end_matches('/'));
    let part = reqwest::multipart::Part::bytes(wav_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", if stt_provider == "groq" { "whisper-large-v3" } else { "whisper-1" });

    let resp = client.post(&url)
        .header("Authorization", format!("Bearer {}", stt_key))
        .multipart(form)
        .send()
        .await?;

    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        eprintln!("STT API error: {}", text);
        let _ = app_handle.emit("voice-state", json!({ "state": "listening" }));
        reset_session_to_listening().await;
        return Ok(());
    }

    let res_json: serde_json::Value = resp.json().await?;
    let transcribed_text = res_json["text"].as_str().unwrap_or("").trim().to_string();
    
    if transcribed_text.is_empty() {
        let _ = app_handle.emit("voice-state", json!({ "state": "listening" }));
        reset_session_to_listening().await;
        return Ok(());
    }

    // Display user transcription bubble in GUI
    let _ = app_handle.emit("voice-transcribed", json!({
        "session_id": session_id,
        "content": transcribed_text.clone(),
    }));

    // Stop command VAD bypass: if user spoken input is a stop command, cancel parallel background task
    {
        let cleaned = transcribed_text.trim().to_lowercase()
            .replace(|c: char| c.is_ascii_punctuation(), "");
        let stop_words = ["stop", "cancel", "detente", "deten", "parar", "alto", "espera", "parala", "arrête", "stopp"];
        if stop_words.iter().any(|&w| cleaned == w || cleaned.contains(w)) {
            state.stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(s) = state.answer_sender.lock().await.take() {
                let _ = s.try_send("cancelled".to_string());
            }
        }
    }
    
    // Save user message to history & db
    {
        let mut history = state.history.lock().await;
        history.push(ChatMessage::text("user", &transcribed_text));
        if let Ok(conn) = db::open(&*state.storage_dir.lock().await) {
            let _ = db::insert_message(&conn, &session_id, "user", &transcribed_text);
        }
    }

    if *cancel_rx.borrow() { return Ok(()); }

    // Stream LLM chat completion
    let history_payload = {
        let history = state.history.lock().await;
        history.clone()
    };

    let llm_url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
    let mut request = client.post(&llm_url);
    if provider_id == "openai" {
        request = request.header("Authorization", format!("Bearer {}", api_key));
    } else if provider_id == "ollama" {
        // ollama auth not needed
    } else {
        request = request.header("Authorization", format!("Bearer {}", api_key));
    }

    let request_body = json!({
        "model": model_name,
        "messages": history_payload,
        "stream": true,
        "max_tokens": 150
    });

    let mut response = match request.json(&request_body).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("LLM request failed: {}", e);
            let _ = app_handle.emit("voice-state", json!({ "state": "listening" }));
            reset_session_to_listening().await;
            return Ok(());
        }
    };

    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        eprintln!("LLM error: {}", text);
        let _ = app_handle.emit("voice-state", json!({ "state": "listening" }));
        reset_session_to_listening().await;
        return Ok(());
    }

    let (tts_tx, mut tts_rx) = tokio::sync::mpsc::channel::<String>(20);
    let app_handle_clone = app_handle.clone();
    let api_key_clone = api_key.clone();
    let provider_id_clone = provider_id.clone();
    let mut cancel_rx_clone = cancel_rx.clone();

    // Spawn companion task to synthesize and stream bot audio chunks
    let tts_worker = tokio::spawn(async move {
        let mut is_first = true;
        while let Some(sentence) = tts_rx.recv().await {
            if *cancel_rx_clone.borrow() { break; }
            let parts = split_sentence_for_tts(&sentence);
            for part in parts {
                if *cancel_rx_clone.borrow() { break; }
                let audio_bytes = match synthesize_tts(&part, &provider_id_clone, &api_key_clone).await {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("TTS synthesis failed: {}", e);
                        continue;
                    }
                };
                if *cancel_rx_clone.borrow() { break; }
                if is_first {
                    is_first = false;
                    let _ = app_handle_clone.emit("voice-state", json!({ "state": "speaking" }));
                    let active = active_session_state().lock().await;
                    if let Some(ref active_session) = *active {
                        active_session.lock().await.state = VoiceState::Speaking;
                    }
                }
                let base64_audio = base64_encode(&audio_bytes);
                let _ = app_handle_clone.emit("bot-audio-chunk", json!({ "chunk": base64_audio }));
            }
        }

        // Return to listening
        if !*cancel_rx_clone.borrow() {
            let _ = app_handle_clone.emit("voice-state", json!({ "state": "listening" }));
            reset_session_to_listening().await;
        }
    });

    let mut buffer = String::new();
    let mut full_response_text = String::new();
    let mut sentence_accumulator = String::new();

    loop {
        if *cancel_rx.borrow() { break; }
        let chunk = match response.chunk().await {
            Ok(Some(c)) => c,
            Ok(None) => break,
            Err(_) => break,
        };
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        while let Some(idx) = buffer.find('\n') {
            let line = buffer[..idx].trim().to_string();
            buffer = buffer[idx + 1..].to_string();

            if line.starts_with("data: ") {
                let data_str = &line[6..];
                if data_str == "[DONE]" { break; }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(data_str) {
                    if let Some(content) = val["choices"][0]["delta"]["content"].as_str() {
                        full_response_text.push_str(content);
                        sentence_accumulator.push_str(content);

                        if content.contains('.') || content.contains('?') || content.contains('!') || content.contains('\n') {
                            let sentence = sentence_accumulator.trim().to_string();
                            if !sentence.is_empty() {
                                sentence_accumulator.clear();
                                let _ = tts_tx.send(sentence).await;
                            }
                        }
                    }
                }
            }
        }
    }

    let remaining_sentence = sentence_accumulator.trim().to_string();
    if !remaining_sentence.is_empty() {
        let _ = tts_tx.send(remaining_sentence).await;
    }

    drop(tts_tx); // close channel so worker exits after processing all sentences
    let _ = tts_worker.await;

    if !*cancel_rx.borrow() && !full_response_text.is_empty() {
        // Save assistant reply to history & db
        {
            let mut history = state.history.lock().await;
            history.push(ChatMessage::text("assistant", &full_response_text));
            if let Ok(conn) = db::open(&*state.storage_dir.lock().await) {
                let _ = db::insert_message(&conn, &session_id, "assistant", &full_response_text);
            }
        }
        // Render bubble in chat
        let _ = app_handle.emit("agent-done", json!({
            "session_id": session_id,
            "content": full_response_text,
        }));
    }

    Ok(())
}

async fn reset_session_to_listening() {
    let active = active_session_state().lock().await;
    if let Some(ref active_session) = *active {
        let mut session = active_session.lock().await;
        session.state = VoiceState::Listening;
        session.audio_buffer.clear();
        session.speech_samples = 0;
        session.silence_samples = 0;
        session.is_user_speaking = false;
    }
}

async fn synthesize_tts(text: &str, provider_id: &str, api_key: &str) -> anyhow::Result<Vec<u8>> {
    let client = reqwest::Client::new();
    let cleaned = text.trim();
    if cleaned.is_empty() {
        return Ok(vec![]);
    }

    if provider_id == "openai" && !api_key.is_empty() {
        let url = "https://api.openai.com/v1/audio/speech";
        let body = json!({
            "model": "tts-1",
            "input": cleaned,
            "voice": "alloy",
            "response_format": "mp3"
        });
        let resp = client.post(url)
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&body)
            .send()
            .await?;
        if resp.status().is_success() {
            let bytes = resp.bytes().await?;
            return Ok(bytes.to_vec());
        }
    }

    // Google Translate TTS fallback
    let lang = detect_language(cleaned);
    let encoded = url_encode(cleaned);
    let url = format!(
        "https://translate.google.com/translate_tts?ie=UTF-8&tl={}&client=tw-ob&q={}",
        lang, encoded
    );
    let resp = client.get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .send()
        .await?;
    if resp.status().is_success() {
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    } else {
        anyhow::bail!("Google TTS failed with status: {}", resp.status())
    }
}

fn detect_language(text: &str) -> &str {
    let lower = text.to_lowercase();
    if lower.contains("hola") || lower.contains("gracias") || lower.contains("cómo") || lower.contains("esta") {
        "es"
    } else if lower.contains("bonjour") || lower.contains("merci") || lower.contains("oui") {
        "fr"
    } else if lower.contains("hallo") || lower.contains("danke") || lower.contains("bitte") {
        "de"
    } else if lower.contains("ciao") || lower.contains("grazie") {
        "it"
    } else if lower.contains("obrigado") || lower.contains("bom dia") {
        "pt"
    } else if lower.chars().any(|c| (0x3040..=0x30FF).contains(&(c as u32)) || (0x4E00..=0x9FFF).contains(&(c as u32))) {
        "ja"
    } else {
        "en"
    }
}

fn url_encode(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            b' ' => {
                encoded.push('+');
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

fn split_sentence_for_tts(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.len() + word.len() + 1 > 150 {
            if !current.is_empty() {
                parts.push(current.clone());
                current.clear();
            }
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).map(|&b| b as usize).unwrap_or(0);
        let b2 = chunk.get(2).map(|&b| b as usize).unwrap_or(0);
        let value = (b0 << 16) | (b1 << 8) | b2;
        result.push(ALPHABET[(value >> 18) & 0x3F] as char);
        result.push(ALPHABET[(value >> 12) & 0x3F] as char);
        if chunk.len() > 1 {
            result.push(ALPHABET[(value >> 6) & 0x3F] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[value & 0x3F] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [0u8; 256];
    for (i, &c) in ALPHABET.iter().enumerate() {
        lookup[c as usize] = i as u8;
    }
    let input = input.trim();
    if input.len() % 4 != 0 {
        return None;
    }
    let mut result = Vec::with_capacity(input.len() / 4 * 3);
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = lookup[bytes[i] as usize] as usize;
        let b1 = lookup[bytes[i+1] as usize] as usize;
        let (b2, b2_valid) = if bytes[i+2] == b'=' { (0, false) } else { (lookup[bytes[i+2] as usize] as usize, true) };
        let (b3, b3_valid) = if bytes[i+3] == b'=' { (0, false) } else { (lookup[bytes[i+3] as usize] as usize, true) };
        
        let value = (b0 << 18) | (b1 << 12) | (b2 << 6) | b3;
        result.push(((value >> 16) & 0xFF) as u8);
        if b2_valid {
            result.push(((value >> 8) & 0xFF) as u8);
        }
        if b3_valid {
            result.push((value & 0xFF) as u8);
        }
        i += 4;
    }
    Some(result)
}

fn write_wav_file(path: &std::path::Path, samples: &[i16]) -> Result<(), std::io::Error> {
    use std::fs::File;
    use std::io::Write;
    
    let mut file = File::create(path)?;
    let spec_channels = 1u16;
    let spec_sample_rate = 16000u32;
    let spec_bits_per_sample = 16u16;
    
    let data_size = samples.len() * 2;
    let file_size = 36 + data_size;
    
    file.write_all(b"RIFF")?;
    file.write_all(&(file_size as u32).to_le_bytes())?;
    file.write_all(b"WAVE")?;
    
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&spec_channels.to_le_bytes())?;
    file.write_all(&spec_sample_rate.to_le_bytes())?;
    
    let byte_rate = spec_sample_rate * spec_channels as u32 * (spec_bits_per_sample / 8) as u32;
    file.write_all(&byte_rate.to_le_bytes())?;
    
    let block_align = spec_channels * (spec_bits_per_sample / 8);
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&spec_bits_per_sample.to_le_bytes())?;
    
    file.write_all(b"data")?;
    file.write_all(&(data_size as u32).to_le_bytes())?;
    
    for &sample in samples {
        file.write_all(&sample.to_le_bytes())?;
    }
    
    Ok(())
}
