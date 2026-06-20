use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use std::time::Duration;
use super::app::{App, AppMode, COMMANDS};
use crate::device;

#[allow(dead_code)]
pub enum AppEvent {
    None,
    Submit(String),
    SwitchModel(String),
    Stop,
    Continue,
    Quit,
    ToggleModelSelect,
    ToggleTaskManager,
    ToggleHelp,
    ToggleConversationList,
    ToggleConversationRename,
    ToggleConversationDeleteConfirm(String),
    EnterConversation,
    DeleteConversation,
    RenameConversation,
    ConfirmDownload(String),
    CancelDownload,
    ConfirmDelete,
    CancelDelete,
    ToggleApiKeyInput(String),
    SetApiKey(String),
    ModelsFetched(Vec<crate::tui::model_fetch::FetchedModel>),
    DownloadModel(String),
    DeleteModel(String, String),
    DeleteTask(String),
    ToggleTask(String),
    Command(String),
    LoadConversation(String),
    ToggleThinkingExpand,
    AddProviderKey(String, String),
    DeleteProviderKey,
    OnboardingNext,
    OnboardingBack,
    OnboardingSkip,
    OnboardingValidateKey,
    OnboardingDone,
    ClarificationAnswer(String),
    SystemMessage(String),
    ImportFinished(String),
}

pub fn handle_events(app: &mut App) -> Result<AppEvent> {
    if !event::poll(Duration::from_millis(16))? {
        return Ok(AppEvent::None);
    }

    let evt = event::read()?;

    if let Event::Resize(_, _) = evt {
        let (primary, all) = crate::device::capture_all_screens();
        let changed = all.len() != app.device_screens.len()
            || all.iter().zip(app.device_screens.iter()).any(|(a, b)| {
                a.width != b.width || a.height != b.height || a.is_primary != b.is_primary
            });
        if changed {
            app.device_screens = all;
            app.device_screen_w = primary.width;
            app.device_screen_h = primary.height;
            app.device_screen_scale = primary.scale_factor;
            app.device_changed = true;
        }
        return Ok(AppEvent::None);
    }

    if let Event::Mouse(mouse) = evt {
        use crossterm::event::MouseEventKind;

        let col = mouse.column;
        let row = mouse.row;

        // Estimate panel area based on terminal size and panel settings
        let term_size = crossterm::terminal::size().unwrap_or((80, 24));
        let term_w = term_size.0;
        let panel_start = term_w.saturating_sub(app.panel_width);

        let over_panel = app.panel_visible 
            && app.panel_width > 0 
            && col >= panel_start 
            && col < term_w;
        let over_chat = col < panel_start;

        match mouse.kind {
            MouseEventKind::ScrollDown => {
                if over_panel {
                    app.panel_scroll = app.panel_scroll.saturating_add(3);
                } else {
                    app.scroll_offset = app.scroll_offset.saturating_add(3);
                }
                return Ok(AppEvent::None);
            }
            MouseEventKind::ScrollUp => {
                if over_panel {
                    app.panel_scroll = app.panel_scroll.saturating_sub(3);
                } else {
                    let total_lines: usize = app.messages.iter().map(|m| {
                        m.content.lines().count().max(1) + 4
                    }).sum();
                    let term_h = crossterm::terminal::size().map(|(_, h)| h as usize).unwrap_or(24);
                    let visible = term_h.saturating_sub(8);
                    let max_skip = total_lines.saturating_sub(visible);
                    app.scroll_offset = app.scroll_offset.min(max_skip).saturating_sub(3);
                }
                return Ok(AppEvent::None);
            }
            _ => {}
        }
        return Ok(AppEvent::None);
    }

    if let Event::Key(key) = evt {
        if key.kind != event::KeyEventKind::Press {
            return Ok(AppEvent::None);
        }

        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(AppEvent::Quit);
        }

        match app.mode {
            AppMode::Onboarding => {
                return handle_onboarding_event(app, key);
            }
            AppMode::Chat => match key.code {
                KeyCode::Esc => {
                    if app.thinking {
                        return Ok(AppEvent::Stop);
                    } else {
                        app.confirm_quit = false;
                        app.status = "Ready".to_string();
                    }
                }
                KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if app.confirm_quit {
                        return Ok(AppEvent::Quit);
                    } else {
                        app.confirm_quit = true;
                        app.status = "Press Ctrl+Q again to quit".to_string();
                    }
                }
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) && app.thinking => {
                    return Ok(AppEvent::ToggleThinkingExpand);
                }
                KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.mode = AppMode::ModelSelect;
                    app.model_cursor = 0;
                }
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.mode = AppMode::ConversationList;
                }
                KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.mode = AppMode::TaskManager;
                }
                KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.mode = AppMode::Help;
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.panel_visible = !app.panel_visible;
                }
                KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if app.panel_width > 16 {
                        app.panel_width -= 2;
                    }
                }
                KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if app.panel_width < 60 {
                        app.panel_width += 2;
                    }
                }
                KeyCode::Enter => {
                    if !app.input.is_empty() && !app.thinking {
                        let msg = app.input.clone();
                        app.input.clear();
                        app.cursor_pos = 0;
                        return Ok(AppEvent::Submit(msg));
                    }
                }
                KeyCode::F(5) if !app.thinking => {
                    return Ok(AppEvent::Submit("__VOICE__:5".to_string()));
                }
                KeyCode::Backspace => {
                    if app.cursor_pos > 0 {
                        app.cursor_pos -= 1;
                        app.input.remove(app.cursor_pos);
                    }
                    if app.input.is_empty() {
                        app.mode = AppMode::Chat;
                    }
                }
                KeyCode::Left => {
                    if app.cursor_pos > 0 { app.cursor_pos -= 1; }
                }
                KeyCode::Right => {
                    if app.cursor_pos < app.input.len() { app.cursor_pos += 1; }
                }
                KeyCode::Up => {
                    let total_lines: usize = app.messages.iter().map(|m| {
                        m.content.lines().count().max(1) + 4
                    }).sum();
                    let term_h = crossterm::terminal::size().map(|(_, h)| h as usize).unwrap_or(24);
                    let visible = term_h.saturating_sub(8);
                    let max_skip = total_lines.saturating_sub(visible);
                    app.scroll_offset = app.scroll_offset.min(max_skip).saturating_sub(3);
                }
                KeyCode::Down => {
                    app.scroll_offset = app.scroll_offset.saturating_add(3);
                }
                KeyCode::Tab => {
                    if app.input.starts_with('/') {
                        let filtered = filtered_commands(&app.input);
                        if let Some((cmd, _)) = filtered.get(app.command_menu_cursor) {
                            let cmd = cmd.to_string();
                            app.input.clear();
                            app.cursor_pos = 0;
                            app.mode = AppMode::Chat;
                            return Ok(AppEvent::Command(cmd));
                        }
                    }
                }
                KeyCode::Char(c) => {
                    app.confirm_quit = false;
                    app.input.insert(app.cursor_pos, c);
                    app.cursor_pos += 1;
                    if app.input.starts_with('/') {
                        app.mode = AppMode::CommandMenu;
                        app.command_menu_cursor = 0;
                    }
                }
                _ => {}
            },

            AppMode::CommandMenu => match key.code {
                KeyCode::Esc => {
                    app.mode = AppMode::Chat;
                    app.input.clear();
                    app.cursor_pos = 0;
                }
                KeyCode::Backspace => {
                    if app.cursor_pos > 0 {
                        app.cursor_pos -= 1;
                        app.input.remove(app.cursor_pos);
                    }
                    if app.input.is_empty() {
                        app.mode = AppMode::Chat;
                    }
                    app.command_menu_cursor = 0;
                }
                KeyCode::Char(c) => {
                    app.input.insert(app.cursor_pos, c);
                    app.cursor_pos += 1;
                    app.command_menu_cursor = 0;
                }
                KeyCode::Up => {
                    let count = filtered_commands(&app.input).len();
                    if app.command_menu_cursor > 0 { app.command_menu_cursor -= 1; }
                    else { app.command_menu_cursor = count.saturating_sub(1); }
                }
                KeyCode::Down => {
                    let count = filtered_commands(&app.input).len();
                    if app.command_menu_cursor + 1 < count {
                        app.command_menu_cursor += 1;
                    } else {
                        app.command_menu_cursor = 0;
                    }
                }
                KeyCode::Enter | KeyCode::Tab => {
                    let filtered = filtered_commands(&app.input);
                    if let Some((cmd, _)) = filtered.get(app.command_menu_cursor) {
                        let cmd = cmd.to_string();
                        app.input.clear();
                        app.cursor_pos = 0;
                        app.mode = AppMode::Chat;
                        return Ok(AppEvent::Command(cmd));
                    } else if app.input.starts_with('/') {
                        let cmd = app.input.clone();
                        app.input.clear();
                        app.cursor_pos = 0;
                        app.mode = AppMode::Chat;
                        return Ok(AppEvent::Command(cmd));
                    }
                }
                _ => {}
            },

            AppMode::ModelSelect => match key.code {
                KeyCode::Esc => {
                    app.mode = AppMode::Chat;
                    app.model_search.clear();
                    app.model_cursor = 0;
                    app.model_scroll_offset = 0;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(AppEvent::Quit);
                }
                KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.mode = AppMode::Chat;
                }
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.mode = AppMode::ConversationList;
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let sections = crate::tui::model_list::build_model_list_grouped(app);
                    let mut idx = 0usize;
                    let mut selected_entry = None;
                    'outer: for section in &sections {
                        for pg in &section.providers {
                            for entry in &pg.models {
                                if idx == app.model_cursor {
                                    selected_entry = Some(entry.clone());
                                    break 'outer;
                                }
                                idx += 1;
                            }
                        }
                    }
                    if let Some(entry) = selected_entry {
                        if (entry.provider_id == "ollama" || entry.provider_id == "localai") && entry.is_installed {
                            app.pending_delete_model = entry.name;
                            app.pending_delete_provider = entry.provider_id;
                            app.mode = AppMode::ModelDeleteConfirm;
                        }
                    }
                }
                KeyCode::Enter => {
                    let sections = crate::tui::model_list::build_model_list_grouped(app);
                    let mut idx = 0usize;
                    let mut selected_name: Option<String> = None;
                    'outer: for section in &sections {
                        for pg in &section.providers {
                            for entry in &pg.models {
                                if idx == app.model_cursor {
                                    selected_name = Some(entry.name.clone());
                                    break 'outer;
                                }
                                idx += 1;
                            }
                        }
                    }
                    if let Some(name) = selected_name {
                        app.model_search.clear();
                        app.model_cursor = 0;
                        app.model_scroll_offset = 0;
                        app.mode = AppMode::Chat;
                        return Ok(AppEvent::SwitchModel(name));
                    }
                }
                KeyCode::Backspace => {
                    app.model_search.pop();
                    app.model_cursor = 0;
                }
                KeyCode::Up => {
                    if app.model_cursor > 0 { app.model_cursor -= 1; }
                }
                KeyCode::Down => {
                    app.model_cursor += 1;
                }
                KeyCode::Char(c) => {
                    app.confirm_quit = false;
                    app.model_search.push(c);
                    app.model_cursor = 0;
                }
                _ => {}
            },

            AppMode::ConversationList => match key.code {
                KeyCode::Esc => {
                    app.mode = AppMode::Chat;
                }
                KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if let Some(conv) = app.conversations.get(app.conversation_cursor) {
                        app.rename_input = conv.name.clone();
                        if app.rename_input.is_empty() {
                            app.rename_input = conv.preview.clone();
                        }
                        app.cursor_pos = app.rename_input.len();
                        app.mode = AppMode::ConversationRename;
                    }
                }
                KeyCode::Char('m') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.mode = AppMode::ModelSelect;
                }
                KeyCode::Enter => {
                    if let Some(session) = app.conversations.get(app.conversation_cursor) {
                        let id = session.id.clone();
                        return Ok(AppEvent::LoadConversation(id));
                    }
                }
                KeyCode::Backspace => {
                    if app.conversation_cursor < app.conversations.len() {
                        return Ok(AppEvent::ToggleConversationDeleteConfirm(
                            app.conversations[app.conversation_cursor].id.clone()
                        ));
                    }
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    if app.conversation_cursor < app.conversations.len() {
                        return Ok(AppEvent::ToggleConversationDeleteConfirm(
                            app.conversations[app.conversation_cursor].id.clone()
                        ));
                    }
                }
                KeyCode::Up => {
                    if app.conversation_cursor > 0 { app.conversation_cursor -= 1; }
                }
                KeyCode::Down => {
                    if app.conversation_cursor < app.conversations.len().saturating_sub(1) {
                        app.conversation_cursor += 1;
                    }
                }
                _ => {}
            },

            AppMode::TaskManager => match key.code {
                KeyCode::Esc => {
                    app.mode = AppMode::Chat;
                }
                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.mode = AppMode::Chat;
                }
                KeyCode::Enter | KeyCode::Char('p') => {
                    if let Some(task) = app.tasks.get(app.task_cursor) {
                        return Ok(AppEvent::ToggleTask(task.id.clone()));
                    }
                }
                KeyCode::Backspace | KeyCode::Char('d') => {
                    if app.task_cursor < app.tasks.len() {
                        return Ok(AppEvent::DeleteTask(app.tasks[app.task_cursor].id.clone()));
                    }
                }
                KeyCode::Up => {
                    if app.task_cursor > 0 { app.task_cursor -= 1; }
                }
                KeyCode::Down => {
                    if app.task_cursor < app.tasks.len().saturating_sub(1) {
                        app.task_cursor += 1;
                    }
                }
                _ => {}
            },

            AppMode::Help => match key.code {
                KeyCode::Esc => {
                    app.mode = AppMode::Chat;
                }
                KeyCode::Char('h') | KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.mode = AppMode::Chat;
                }
                _ => {}
            },

            AppMode::ConversationRename => match key.code {
                KeyCode::Esc => {
                    app.mode = AppMode::ConversationList;
                    app.rename_input.clear();
                    app.cursor_pos = 0;
                }
                KeyCode::Enter => {
                    if !app.rename_input.is_empty() {
                        return Ok(AppEvent::RenameConversation);
                    }
                }
                KeyCode::Backspace => {
                    if app.cursor_pos > 0 {
                        app.cursor_pos -= 1;
                        app.rename_input.remove(app.cursor_pos);
                    }
                }
                KeyCode::Left => {
                    if app.cursor_pos > 0 { app.cursor_pos -= 1; }
                }
                KeyCode::Right => {
                    if app.cursor_pos < app.rename_input.len() { app.cursor_pos += 1; }
                }
                KeyCode::Char(c) => {
                    app.rename_input.insert(app.cursor_pos, c);
                    app.cursor_pos += 1;
                }
                _ => {}
            },

            AppMode::ConversationDeleteConfirm => match key.code {
                KeyCode::Esc => {
                    app.mode = AppMode::Chat;
                }
                KeyCode::Char('y') | KeyCode::Enter => {
                    return Ok(AppEvent::DeleteConversation);
                }
                KeyCode::Char('n') | KeyCode::Char('q') | KeyCode::Char('d') => {
                    app.mode = AppMode::Chat;
                }
                _ => {}
            },

            AppMode::DownloadConfirm => match key.code {
                KeyCode::Esc | KeyCode::Char('n') => {
                    app.mode = AppMode::ModelSelect;
                }
                KeyCode::Enter | KeyCode::Char('y') => {
                    let model = app.pending_download_model.clone();
                    app.mode = AppMode::Chat;
                    return Ok(AppEvent::DownloadModel(model));
                }
                _ => {}
            },

            AppMode::ModelDeleteConfirm => match key.code {
                KeyCode::Esc | KeyCode::Char('n') => {
                    app.mode = AppMode::ModelSelect;
                    app.pending_delete_model.clear();
                    app.pending_delete_provider.clear();
                }
                KeyCode::Enter | KeyCode::Char('y') => {
                    let model = app.pending_delete_model.clone();
                    let provider = app.pending_delete_provider.clone();
                    app.mode = AppMode::Chat;
                    app.pending_delete_model.clear();
                    app.pending_delete_provider.clear();
                    return Ok(AppEvent::DeleteModel(model, provider));
                }
                _ => {}
            },

            AppMode::KeyManager => match key.code {
                KeyCode::Esc => {
                    app.mode = AppMode::Chat;
                    app.key_manager_provider.clear();
                    app.key_manager_cursor = 0;
                }
                KeyCode::Tab => {
                    use crate::tui::model_fetch::PROVIDERS;
                    let providers: Vec<&str> = PROVIDERS.iter().map(|p| p.id).collect();
                    let cur = providers.iter().position(|&p| p == app.key_manager_provider.as_str()).unwrap_or(0);
                    let next = (cur + 1) % providers.len();
                    app.key_manager_provider = providers[next].to_string();
                    app.key_manager_cursor = 0;
                    app.key_manager_adding = false;
                    app.key_manager_input.clear();
                }
                KeyCode::Char('a') if !app.key_manager_adding => {
                    app.key_manager_adding = true;
                    app.key_manager_input.clear();
                }
                KeyCode::Char('d') if !app.key_manager_adding && !app.key_manager_provider.is_empty() => {
                    return Ok(AppEvent::DeleteProviderKey);
                }
                KeyCode::Up if !app.key_manager_adding => {
                    if app.key_manager_cursor > 0 { app.key_manager_cursor -= 1; }
                }
                KeyCode::Down if !app.key_manager_adding => {
                    let count = app.provider_keys_multi
                        .get(&app.key_manager_provider)
                        .map(|v| v.len()).unwrap_or(0);
                    if app.key_manager_cursor + 1 < count { app.key_manager_cursor += 1; }
                }
                KeyCode::Enter if app.key_manager_adding => {
                    if !app.key_manager_input.is_empty() {
                        return Ok(AppEvent::AddProviderKey(
                            app.key_manager_provider.clone(),
                            app.key_manager_input.clone(),
                        ));
                    }
                }
                KeyCode::Backspace if app.key_manager_adding => {
                    app.key_manager_input.pop();
                }
                KeyCode::Char(c) if app.key_manager_adding => {
                    app.key_manager_input.push(c);
                }
                _ => {}
            },

            AppMode::Clarification => match key.code {
                KeyCode::Esc => {
                    if let Some(tx) = app.clarification_tx.take() {
                        let _ = tx.try_send("cancelled".to_string());
                    }
                    app.clarification_question = None;
                    app.clarification_options.clear();
                    app.mode = AppMode::Chat;
                }
                KeyCode::Up => {
                    if app.clarification_cursor > 0 { app.clarification_cursor -= 1; }
                }
                KeyCode::Down => {
                    if app.clarification_cursor + 1 < app.clarification_options.len() {
                        app.clarification_cursor += 1;
                    }
                }
                KeyCode::Enter => {
                    let selected = app.clarification_options.get(app.clarification_cursor).cloned().unwrap_or_default();
                    let is_custom = selected.to_lowercase().contains("custom") || selected.to_lowercase().contains("other");
                    if is_custom {
                        app.mode = AppMode::Chat;
                        app.clarification_question = None;
                        app.clarification_options.clear();
                    } else {
                        app.push_message("user", &selected);
                        app.clarification_question = None;
                        app.clarification_options.clear();
                        app.mode = AppMode::Chat;
                        app.start_thinking();
                        return Ok(AppEvent::ClarificationAnswer(selected));
                    }
                }
                _ => {}
            },

            AppMode::ProviderSetup => match key.code {
                KeyCode::Esc => { app.mode = AppMode::Chat; }
                _ => {}
            },

            AppMode::ApiKeyInput => match key.code {
                KeyCode::Esc => {
                    app.mode = AppMode::ModelSelect;
                    app.api_key_input.clear();
                }
                KeyCode::Enter => {
                    if !app.api_key_input.is_empty() {
                        let key = app.api_key_input.clone();
                        app.api_key_input.clear();
                        app.mode = AppMode::Chat;
                        return Ok(AppEvent::SetApiKey(key));
                    }
                }
                KeyCode::Backspace => { app.api_key_input.pop(); }
                KeyCode::Char(c) => { app.api_key_input.push(c); }
                _ => {}
            },
        }
    }

    Ok(AppEvent::None)
}

pub fn handle_onboarding_event(app: &mut App, key: event::KeyEvent) -> Result<AppEvent> {
    use crossterm::event::{KeyCode, KeyModifiers};
    use crate::tui::onboarding::{OnboardingStep, ProviderFocus, ONBOARDING_PROVIDERS};

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Ok(AppEvent::Quit);
    }

    let ob = match app.onboarding.as_mut() {
        Some(o) => o,
        None => return Ok(AppEvent::None),
    };

    match ob.step {
        OnboardingStep::Welcome => {
            match key.code {
                KeyCode::Enter => {
                    ob.step = OnboardingStep::Storage;
                    ob.status_line.clear();
                }
                KeyCode::Esc | KeyCode::Char('s') | KeyCode::F(1) => {
                    return Ok(AppEvent::OnboardingSkip);
                }
                _ => {}
            }
        }
        OnboardingStep::Storage => {
            match key.code {
                KeyCode::Up => {
                    ob.storage_cursor = ob.storage_cursor.saturating_sub(1);
                }
                KeyCode::Down => {
                    if ob.storage_cursor < 1 {
                        ob.storage_cursor += 1;
                    }
                }
                KeyCode::Enter => {
                    ob.chosen_storage = if ob.storage_cursor == 0 {
                        crate::config::StorageLocation::Portable
                    } else {
                        crate::config::StorageLocation::Global
                    };
                    ob.step = OnboardingStep::Provider;
                    ob.status_line.clear();
                }
                KeyCode::Esc => {
                    ob.step = OnboardingStep::Welcome;
                    ob.status_line.clear();
                }
                _ => {}
            }
        }
        OnboardingStep::Provider => {
            match key.code {
                KeyCode::Tab => {
                    let is_local = ob.current_provider().map(|p| p.id == "ollama" || p.id == "localai").unwrap_or(false);
                    ob.provider_focus = match ob.provider_focus {
                        ProviderFocus::ProviderList => {
                            if is_local {
                                ProviderFocus::OllamaModelList
                            } else {
                                ProviderFocus::ModelList
                            }
                        }
                        ProviderFocus::ModelList => {
                            ProviderFocus::ApiKeyInput
                        }
                        ProviderFocus::ApiKeyInput => {
                            ProviderFocus::ProviderList
                        }
                        ProviderFocus::OllamaModelList => {
                            ProviderFocus::ProviderList
                        }
                    };
                }
                KeyCode::Up => {
                    match ob.provider_focus {
                        ProviderFocus::ProviderList => {
                            ob.provider_cursor = ob.provider_cursor.saturating_sub(1);
                            ob.model_cursor = 0;
                            ob.ollama_fetched = false;
                        }
                        ProviderFocus::ModelList => {
                            ob.model_cursor = ob.model_cursor.saturating_sub(1);
                        }
                        ProviderFocus::OllamaModelList => {
                            ob.model_cursor = ob.model_cursor.saturating_sub(1);
                        }
                        _ => {}
                    }
                }
                KeyCode::Down => {
                    match ob.provider_focus {
                        ProviderFocus::ProviderList => {
                            if ob.provider_cursor + 1 < ONBOARDING_PROVIDERS.len() {
                                ob.provider_cursor += 1;
                                ob.model_cursor = 0;
                                ob.ollama_fetched = false;
                            }
                        }
                        ProviderFocus::ModelList => {
                            let max = ob.models_for_current_provider().len().saturating_sub(1);
                            if ob.model_cursor < max {
                                ob.model_cursor += 1;
                            }
                        }
                        ProviderFocus::OllamaModelList => {
                            let max = ob.ollama_models.len().saturating_sub(1);
                            if ob.model_cursor < max {
                                ob.model_cursor += 1;
                            }
                        }
                        _ => {}
                    }
                }
                KeyCode::Backspace => {
                    if ob.provider_focus == ProviderFocus::ApiKeyInput {
                        ob.api_key_input.pop();
                    }
                }
                KeyCode::Char(c) => {
                    if ob.provider_focus == ProviderFocus::ApiKeyInput {
                        ob.api_key_input.push(c);
                    }
                }
                KeyCode::Enter => {
                    let provider_id = ob.current_provider().map(|p| p.id).unwrap_or("");
                    if provider_id == "ollama" || provider_id == "localai" {
                        ob.commit_provider();
                        ob.step = OnboardingStep::Memory;
                        ob.status_line.clear();
                    } else {
                        return Ok(AppEvent::OnboardingValidateKey);
                    }
                }
                KeyCode::Esc => {
                    if ob.provider_focus != ProviderFocus::ProviderList {
                        ob.provider_focus = ProviderFocus::ProviderList;
                    } else {
                        ob.step = OnboardingStep::Storage;
                        ob.status_line.clear();
                    }
                }
                _ => {}
            }
        }
        OnboardingStep::Memory => {
            match key.code {
                KeyCode::Char(' ') => {
                    ob.remember_sessions = !ob.remember_sessions;
                }
                KeyCode::Enter => {
                    ob.step = OnboardingStep::Done;
                    ob.status_line.clear();
                }
                KeyCode::Esc => {
                    ob.step = OnboardingStep::Provider;
                    ob.status_line.clear();
                }
                _ => {}
            }
        }
        OnboardingStep::Done => {
            match key.code {
                KeyCode::Enter => {
                    return Ok(AppEvent::OnboardingDone);
                }
                KeyCode::Esc => {
                    ob.step = OnboardingStep::Memory;
                    ob.status_line.clear();
                }
                _ => {}
            }
        }
    }

    Ok(AppEvent::None)
}

pub fn filtered_commands(input: &str) -> Vec<(&'static str, &'static str)> {
    let query = input.trim_start_matches('/').to_lowercase();
    COMMANDS.iter()
        .filter(|(cmd, _)| cmd.trim_start_matches('/').contains(query.as_str()))
        .copied()
        .collect()
}

#[allow(dead_code)]
pub fn filtered_models<'a>(models: &'a [String], search: &str) -> Vec<String> {
    let q = search.to_lowercase();
    models.iter()
        .filter(|m| m.to_lowercase().contains(&q))
        .cloned()
        .collect()
}

pub fn filtered_convs<'a>(convs: &'a [super::app::Conversation], search: &str) -> Vec<&'a super::app::Conversation> {
    let q = search.to_lowercase();
    convs.iter()
        .filter(|c| {
            let label = if c.name.is_empty() { &c.preview } else { &c.name };
            label.to_lowercase().contains(&q)
        })
        .collect()
}