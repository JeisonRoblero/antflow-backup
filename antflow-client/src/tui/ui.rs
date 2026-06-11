use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, BorderType, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use super::app::{App, AppMode, APP_VERSION};
use super::events::{filtered_commands, filtered_convs};
    use super::model_list::build_model_list_grouped;

const ACCENT: Color = Color::Rgb(80, 200, 230);
const DIM: Color = Color::Rgb(60, 75, 100);
const ERROR_COLOR: Color = Color::Rgb(255, 90, 90);
const BG: Color = Color::Rgb(8, 14, 28);
const USER_BG: Color = Color::Rgb(22, 34, 62);
const AI_BG: Color = Color::Rgb(12, 22, 42);
const USER_ACCENT: Color = Color::Rgb(70, 100, 200);
const AI_ACCENT: Color = Color::Rgb(40, 110, 190);
const SPINNER: &[&str] = &["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];

fn draw_dim_overlay(f: &mut Frame, area: Rect) {
    let mut lines = Vec::new();
    for _ in 0..area.height {
        let line = Line::from(Span::raw(" ".repeat(area.width as usize)));
        lines.push(line);
    }
    let overlay = Paragraph::new(lines)
        .style(Style::default().bg(Color::Rgb(8, 10, 16)));
    f.render_widget(overlay, area);
}

pub fn draw(f: &mut Frame, app: &App) {
    let size = f.size();

    if !app.splash_done {
        draw_splash(f, app, size);
        return;
    }

    if app.mode == AppMode::Onboarding {
        if let Some(ob) = &app.onboarding {
            draw_onboarding(f, ob, size);
            return;
        }
    }

    let title = format!(" ⚡ Antflow v{} ", APP_VERSION);
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BG))
        .title(Span::styled(title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)));
    f.render_widget(outer, size);

    let inner = Rect {
        x: size.x + 1,
        y: size.y + 1,
        width: size.width.saturating_sub(2),
        height: size.height.saturating_sub(2),
    };

    let top_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

draw_top_bar(f, app, top_chunks[0]);

    let show_panel = app.panel_visible && size.width >= 80;
    let panel_w = if show_panel { app.panel_width } else { 0 };

    let h_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(panel_w)])
        .split(top_chunks[1]);

    let chat_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(h_chunks[0]);

    draw_messages(f, app, chat_chunks[0]);
    draw_input(f, app, chat_chunks[1]);
    draw_bottom_bar(f, app, chat_chunks[2], show_panel, size.width);
    if show_panel {
        draw_device_panel(f, app, h_chunks[1], app.panel_scroll);
    }

    let has_overlay = !matches!(app.mode, AppMode::Chat | AppMode::CommandMenu);
    if has_overlay {
        draw_dim_overlay(f, size);
    }
    match app.mode {
        AppMode::ModelSelect => { draw_model_overlay(f, app, size); }
        AppMode::Help => { draw_dim_overlay(f, size); draw_help_overlay(f, size); }
        AppMode::CommandMenu => draw_command_menu(f, app, size),
        AppMode::ConversationList => { draw_dim_overlay(f, size); draw_conversation_list(f, app, size); }
        AppMode::ConversationRename => { draw_dim_overlay(f, size); draw_rename_overlay(f, app, size); }
        AppMode::ConversationDeleteConfirm => { draw_dim_overlay(f, size); draw_delete_confirm(f, app, size); }
        AppMode::TaskManager => { draw_dim_overlay(f, size); draw_task_overlay(f, app, size); }
        AppMode::ApiKeyInput => { draw_dim_overlay(f, size); draw_api_key_overlay(f, app, size); }
        AppMode::DownloadConfirm => { draw_dim_overlay(f, size); draw_download_confirm(f, app, size); }
        AppMode::KeyManager => { draw_dim_overlay(f, size); draw_key_manager(f, app, size); }
        AppMode::Clarification => { draw_dim_overlay(f, size); draw_clarification(f, app, size); }
        _ => {}
    }
}

fn draw_splash(f: &mut Frame, app: &App, area: Rect) {
    let elapsed = app.splash_start.elapsed().as_millis();
    let frame = (elapsed / 120) as usize;
    let pulse = frame % 2 == 0;
    let bolt_color = if pulse { Color::Rgb(255, 220, 0) } else { Color::Rgb(255, 160, 0) };
    let title_color = if pulse { ACCENT } else { Color::Rgb(60, 170, 210) };

    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(ACCENT))
            .style(Style::default().bg(BG)),
        area,
    );

    let cx = area.x + area.width / 2;
    let cy = area.y + area.height / 2;

    let bolt: &[&str] = &[
        "   ████   ",
        "  █████   ",
        " ██████   ",
        "█████████ ",
        "   █████  ",
        "   ████   ",
        "    ██    ",
    ];
    let bolt_start = cy.saturating_sub(5);
    for (i, row) in bolt.iter().enumerate() {
        let rw = row.len() as u16;
        let rx = cx.saturating_sub(rw / 2);
        let ry = bolt_start + i as u16;
        if ry < area.y + area.height.saturating_sub(1) {
            f.render_widget(
                Paragraph::new(Span::styled(*row, Style::default().fg(bolt_color).bg(BG))),
                Rect { x: rx, y: ry, width: rw + 1, height: 1 },
            );
        }
    }

    let title = "A  N  T  F  L  O  W";
    let title_y = bolt_start + bolt.len() as u16 + 1;
    let title_x = cx.saturating_sub(title.len() as u16 / 2);
    if title_y < area.y + area.height.saturating_sub(1) {
        f.render_widget(
            Paragraph::new(Span::styled(
                title,
                Style::default().fg(title_color).bg(BG).add_modifier(Modifier::BOLD),
            )),
            Rect { x: title_x, y: title_y, width: title.len() as u16 + 1, height: 1 },
        );
    }

    let sub = format!("v{}  ·  AI automation agent", APP_VERSION);
    let sub_x = cx.saturating_sub(sub.len() as u16 / 2);
    let sub_y = title_y + 2;
    if sub_y < area.y + area.height.saturating_sub(1) {
        f.render_widget(
            Paragraph::new(Span::styled(
                &sub,
                Style::default().fg(Color::Rgb(35, 50, 80)).bg(BG),
            )),
            Rect { x: sub_x, y: sub_y, width: sub.len() as u16 + 1, height: 1 },
        );
    }
}

fn draw_top_bar(f: &mut Frame, app: &App, area: Rect) {
    let pct = if app.token_limit > 0 {
        (app.tokens_used as f32 / app.token_limit as f32 * 100.0) as u32
    } else { 0 };
    let token_color = if pct > 80 { ERROR_COLOR } else if pct > 50 { Color::Yellow } else { Color::Green };
    let is_cloud = matches!(app.cfg.ai.provider.as_str(),
        "openai"|"anthropic"|"gemini"|"mistral"|"groq"|"together"|"openrouter"|"nvidia");

    let mut spans = vec![
        Span::styled("  provider: ", Style::default().fg(DIM)),
        Span::styled(&app.cfg.ai.provider, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled("  model: ", Style::default().fg(DIM)),
        Span::styled(&app.cfg.ai.model, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("  ctx: ", Style::default().fg(DIM)),
        Span::styled(
            format!("{}/{} ({}%)", app.tokens_used, app.token_limit, pct),
            Style::default().fg(token_color),
        ),
    ];

    if is_cloud {
        let cost = app.tokens_used as f32 * 0.000002;
        spans.push(Span::styled(format!("  ~${:.4}", cost), Style::default().fg(Color::Magenta)));
    }

    // Run directory - truncated from right
    let dir_short: String = {
        let full = &app.run_dir;
        if full.len() > 30 {
            format!("...{}", &full[full.len()-27..])
        } else {
            full.clone()
        }
    };
    spans.push(Span::styled(format!("  {}", dir_short), Style::default().fg(DIM)));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_task_overlay(f: &mut Frame, app: &App, area: Rect) {
    let popup_w = if area.width < 80 { 98 } else { 65 };
    let popup = centered_rect(popup_w, 88, area);
    f.render_widget(Clear, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            " Scheduled Tasks - Esc close ",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(chunks[0]);
    f.render_widget(block, chunks[0]);

    let items: Vec<ListItem> = if app.tasks.is_empty() {
        vec![
            ListItem::new(Line::from("")),
            ListItem::new(Line::from(Span::styled(
                "  No scheduled tasks yet.",
                Style::default().fg(DIM),
            ))),
            ListItem::new(Line::from(Span::styled(
                "  Ask the AI to schedule something for you.",
                Style::default().fg(DIM),
            ))),
        ]
    } else {
        app.tasks.iter().enumerate().map(|(i, t)| {
            let is_selected = i == app.task_cursor;
            let (status_icon, status_color) = if t.enabled {
                ("● ", Color::Green)
            } else {
                ("○ ", ERROR_COLOR)
            };
            ListItem::new(Line::from(vec![
                Span::styled(status_icon, Style::default().fg(status_color)),
                Span::styled(
                    format!("{:<32}", t.prompt),
                    Style::default().fg(if is_selected { Color::White } else { Color::Gray })
                        .add_modifier(if is_selected { Modifier::BOLD } else { Modifier::empty() }),
                ),
                Span::styled(" next: ", Style::default().fg(DIM)),
                Span::styled(t.next_run.clone(), Style::default().fg(ACCENT)),
            ]))
        }).collect()
    };

    let mut state = ListState::default();
    state.select(Some(app.task_cursor.min(app.tasks.len().saturating_sub(1))));

    f.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().bg(Color::Rgb(28, 38, 55)).add_modifier(Modifier::BOLD))
            .highlight_symbol("  "),
        inner,
        &mut state,
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ↑↓", Style::default().fg(ACCENT)),
            Span::styled(" navigate  ", Style::default().fg(DIM)),
            Span::styled("d", Style::default().fg(Color::Red)),
            Span::styled(" delete  ", Style::default().fg(DIM)),
            Span::styled("p", Style::default().fg(ACCENT)),
            Span::styled(" toggle pause  ", Style::default().fg(DIM)),
            Span::styled("Esc", Style::default().fg(DIM)),
            Span::styled(" close", Style::default().fg(DIM)),
        ])),
        chunks[1],
    );
}

fn wrap_str(s: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 { return vec![s.to_string()]; }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in s.split_whitespace() {
        if current.is_empty() {
            current = word.to_string();
        } else if current.len() + 1 + word.len() <= max_width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current.clone());
            current = word.to_string();
        }
    }
    if !current.is_empty() { lines.push(current); }
    if lines.is_empty() { lines.push(String::new()); }
    lines
}

fn draw_messages(f: &mut Frame, app: &App, area: Rect) {
    let border_color = DIM;
    let conv_title = app.conversation_title();
    let chat_label = if conv_title.is_empty() {
        " Chat ".to_string()
    } else {
        format!(" Chat - {} ", conv_title)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(chat_label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let has_real_msgs = app.messages.iter().any(|m| m.role == "user" || m.role == "assistant");

    if !has_real_msgs {
        for msg in app.messages.iter().filter(|m| m.role == "system") {
            let is_err = msg.content.starts_with("✗");
            let is_ok  = msg.content.starts_with("✓") || msg.content.starts_with("⬇");
            let fg = if is_err { Color::Rgb(220, 80, 80) }
                     else if is_ok { Color::Rgb(80, 200, 120) }
                     else { Color::Rgb(120, 130, 160) };
            f.render_widget(
                Paragraph::new(Span::styled(
                    format!("  {}", msg.content),
                    Style::default().fg(fg),
                )),
                Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
            );
        }

        let iw = inner.width as usize;
        let ih = inner.height as usize;
        for row in 0..ih {
            let y_pos = inner.y + row as u16;
            f.render_widget(
                Paragraph::new(Span::styled(" ".repeat(iw), Style::default().bg(BG))),
                Rect { x: inner.x, y: y_pos, width: inner.width, height: 1 },
            );
        }

        let center_y = inner.y + inner.height / 2;
        let bolt = "⚡";
        f.render_widget(
            Paragraph::new(Span::styled(bolt,
                Style::default().fg(Color::Rgb(255, 190, 30)).add_modifier(Modifier::BOLD)))
                .alignment(Alignment::Center),
            Rect { x: inner.x, y: center_y.saturating_sub(3), width: inner.width, height: 1 },
        );

        f.render_widget(
            Paragraph::new(Span::styled(
                "A  N  T  F  L  O  W",
                Style::default()
                    .fg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            )).alignment(Alignment::Center),
            Rect { x: inner.x, y: center_y.saturating_sub(1), width: inner.width, height: 1 },
        );

        f.render_widget(
            Paragraph::new(Span::styled(
                format!("v{}  ·  AI automation agent", APP_VERSION),
                Style::default().fg(Color::Rgb(55, 62, 90)),
            )).alignment(Alignment::Center),
            Rect { x: inner.x, y: center_y + 1, width: inner.width, height: 1 },
        );

        let welcome_rect = Rect {
            x: inner.x + 2,
            y: center_y + 3,
            width: inner.width.saturating_sub(4),
            height: inner.height.saturating_sub(center_y - inner.y + 3).min(5),
        };
        f.render_widget(
            Paragraph::new(app.welcome_message.as_str())
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Rgb(140, 145, 175)))
                .wrap(Wrap { trim: true }),
            welcome_rect,
        );

        return;
    }

    let width = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        match msg.role.as_str() {
            "user" => {
                let bubble_max = (width * 3 / 4).max(30).min(width.saturating_sub(4));
                let content_lines: Vec<&str> = msg.content.lines().collect();
                let pad_left = width.saturating_sub(bubble_max + 4);

                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(pad_left)),
                    Span::styled("  ", Style::default().bg(USER_BG)),
                    Span::styled(" ".repeat(bubble_max), Style::default().bg(USER_BG)),
                    Span::styled("  ", Style::default().bg(USER_BG)),
                ]));

                for content_line in &content_lines {
                    let chunks = wrap_str(content_line, bubble_max.saturating_sub(4));
                    for chunk in chunks {
                        let right_pad = bubble_max.saturating_sub(chunk.len() + 2);
                        lines.push(Line::from(vec![
                            Span::raw(" ".repeat(pad_left)),
                            Span::styled(" ", Style::default().bg(USER_ACCENT)),
                            Span::styled(" ", Style::default().bg(USER_BG)),
                            Span::styled(
                                format!(" {}", chunk),
                                Style::default().fg(Color::Rgb(230, 232, 255)).bg(USER_BG),
                            ),
                            Span::styled(" ".repeat(right_pad + 1), Style::default().bg(USER_BG)),
                            Span::styled("  ", Style::default().bg(USER_BG)),
                        ]));
                    }
                }

                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(pad_left)),
                    Span::styled("  ", Style::default().bg(USER_BG)),
                    Span::styled(" ".repeat(bubble_max), Style::default().bg(USER_BG)),
                    Span::styled("  ", Style::default().bg(USER_BG)),
                ]));

                let ts = &msg.timestamp;
                let ts_pad = width.saturating_sub(ts.len() + 2);
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(ts_pad)),
                    Span::styled(ts.clone(), Style::default().fg(Color::Rgb(70, 75, 110))),
                    Span::raw("  "),
                ]));
                lines.push(Line::from(""));
            }
            "assistant" => {
                let spinner_idx = (app.thinking_secs as usize) % SPINNER.len();
                let spinner = SPINNER[spinner_idx];
                let is_placeholder = msg.content == "▋" && app.thinking;

                if is_placeholder {
                    lines.push(Line::from(vec![
                        Span::styled("  ", Style::default()),
                        Span::styled(
                            format!("{} {}s", spinner, app.thinking_secs),
                            Style::default().fg(Color::Rgb(150, 180, 255)).bg(AI_BG),
                        ),
                    ]));
                } else {
                    let bubble_max = (width * 4 / 5).max(30).min(width.saturating_sub(4));
                    let rendered = render_markdown(&msg.content);

                    lines.push(Line::from(vec![
                        Span::styled(" ", Style::default().bg(AI_ACCENT)),
                        Span::styled(" ", Style::default().bg(AI_BG)),
                        Span::styled(" ".repeat(bubble_max + 2), Style::default().bg(AI_BG)),
                    ]));

                    for rline in &rendered {
                        let mut spans = vec![
                            Span::styled(" ", Style::default().bg(AI_ACCENT)),
                            Span::styled(" ", Style::default().bg(AI_BG)),
                        ];
                        for s in &rline.spans {
                            let mut ns = s.clone();
                            if ns.style.bg.is_none() {
                                ns.style = ns.style.bg(AI_BG);
                            }
                            spans.push(ns);
                        }
                        spans.push(Span::styled("  ", Style::default().bg(AI_BG)));
                        lines.push(Line::from(spans));
                    }

                    lines.push(Line::from(vec![
                        Span::styled(" ", Style::default().bg(AI_ACCENT)),
                        Span::styled(" ", Style::default().bg(AI_BG)),
                        Span::styled(" ".repeat(bubble_max + 2), Style::default().bg(AI_BG)),
                    ]));

                    lines.push(Line::from(vec![
                        Span::raw("   "),
                        Span::styled(msg.timestamp.clone(), Style::default().fg(Color::Rgb(60, 70, 100))),
                    ]));
                }
                lines.push(Line::from(""));
            }
            "system" => {
                let is_error = msg.content.starts_with("✗") || msg.content.starts_with("Error:");
                let is_success = msg.content.starts_with("✓") || msg.content.starts_with("⬇");
                let fg = if is_error { Color::Rgb(255, 100, 100) }
                        else if is_success { Color::Rgb(100, 200, 120) }
                        else { Color::Rgb(140, 150, 180) };

                for (i, sline) in msg.content.lines().enumerate() {
                    let prefix = if i == 0 { "  " } else { "    " };
                    lines.push(Line::from(Span::styled(
                        format!("{}{}", prefix, sline),
                        Style::default().fg(fg),
                    )));
                }
                lines.push(Line::from(""));
            }
            "action" => {
                // Skip - action messages are shown inside the assistant bubble expand
            }
            _ => {}
        }
    }

    let total = lines.len();
    let visible_height = inner.height as usize;
    let max_skip = total.saturating_sub(visible_height);
    let skip = app.scroll_offset.min(max_skip);
    let visible: Vec<Line> = lines.into_iter().skip(skip).collect();

    f.render_widget(
        Paragraph::new(Text::from(visible)).wrap(Wrap { trim: false }),
        inner,
    );
}

/// Lightweight markdown renderer
fn render_markdown(text: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;

    for raw_line in text.lines() {
        if raw_line.starts_with("```") {
            in_code_block = !in_code_block;
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                Style::default().fg(Color::Rgb(180, 180, 100)).bg(Color::Rgb(30, 30, 30)),
            )));
            continue;
        }
        if in_code_block {
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                Style::default().fg(Color::Rgb(200, 200, 120)).bg(Color::Rgb(30, 30, 30)),
            )));
            continue;
        }
        if raw_line.starts_with("### ") {
            lines.push(Line::from(Span::styled(
                raw_line[4..].to_string(),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD).add_modifier(Modifier::UNDERLINED),
            )));
            continue;
        }
        if raw_line.starts_with("## ") {
            lines.push(Line::from(Span::styled(
                raw_line[3..].to_string(),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if raw_line.starts_with("# ") {
            lines.push(Line::from(Span::styled(
                raw_line[2..].to_string(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if raw_line.starts_with("    ") {
            lines.push(Line::from(Span::styled(
                raw_line.to_string(),
                Style::default().fg(Color::Rgb(180, 180, 100)).bg(Color::Rgb(30, 30, 30)),
            )));
            continue;
        }
        if raw_line.starts_with("- ") || raw_line.starts_with("* ") {
            let content = &raw_line[2..];
            let mut spans = vec![Span::styled("  * ", Style::default().fg(ACCENT))];
            spans.extend(parse_inline_spans(content));
            lines.push(Line::from(spans));
            continue;
        }
        if raw_line.starts_with("> ") {
            lines.push(Line::from(vec![
                Span::styled("  | ", Style::default().fg(Color::Magenta)),
                Span::styled(raw_line[2..].to_string(), Style::default().fg(Color::Gray).add_modifier(Modifier::ITALIC)),
            ]));
            continue;
        }
        if raw_line.starts_with("---") || raw_line.starts_with("===") {
            lines.push(Line::from(Span::styled(
                "─".repeat(60),
                Style::default().fg(DIM),
            )));
            continue;
        }
        lines.push(parse_inline(raw_line));
    }
    lines
}

fn parse_inline_spans(text: &str) -> Vec<Span<'static>> {
    parse_inline(text).spans
}

fn parse_inline(text: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let mut remaining = text.to_string();

    while !remaining.is_empty() {
        if let Some(start) = remaining.find("**") {
            if let Some(end) = remaining[start + 2..].find("**") {
                if start > 0 {
                    spans.push(Span::raw(remaining[..start].to_string()));
                }
                spans.push(Span::styled(
                    remaining[start + 2..start + 2 + end].to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                ));
                remaining = remaining[start + 2 + end + 2..].to_string();
                continue;
            }
        }
        if let Some(start) = remaining.find('`') {
            if let Some(end) = remaining[start + 1..].find('`') {
                if start > 0 {
                    spans.push(Span::raw(remaining[..start].to_string()));
                }
                spans.push(Span::styled(
                    remaining[start + 1..start + 1 + end].to_string(),
                    Style::default().fg(Color::Rgb(180, 180, 100)).bg(Color::Rgb(40, 40, 40)),
                ));
                remaining = remaining[start + 1 + end + 1..].to_string();
                continue;
            }
        }
        if let Some(start) = remaining.find('*') {
            if let Some(end) = remaining[start + 1..].find('*') {
                if start > 0 {
                    spans.push(Span::raw(remaining[..start].to_string()));
                }
                spans.push(Span::styled(
                    remaining[start + 1..start + 1 + end].to_string(),
                    Style::default().add_modifier(Modifier::ITALIC),
                ));
                remaining = remaining[start + 1 + end + 1..].to_string();
                continue;
            }
        }
        spans.push(Span::raw(remaining.clone()));
        break;
    }

    Line::from(spans)
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let is_truly_thinking = app.thinking;

    let (title, border_color) = if is_truly_thinking {
        (
            format!(" {} {}s - Esc to stop ", SPINNER[(app.thinking_secs as usize) % SPINNER.len()], app.thinking_secs),
            Color::Yellow,
        )
    } else {
        (" Message ".to_string(), ACCENT)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(title.clone(), Style::default().fg(border_color).add_modifier(Modifier::BOLD)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if is_truly_thinking {
        f.render_widget(
            Paragraph::new(Span::styled("processing...", Style::default().fg(DIM))),
            inner,
        );
    } else {
        let input_display = if app.input.is_empty() {
            Paragraph::new(Span::styled("Type / for commands...", Style::default().fg(DIM)))
        } else {
            Paragraph::new(app.input.as_str())
        };
        f.render_widget(input_display, inner);
        if matches!(app.mode, AppMode::Chat | AppMode::CommandMenu) {
            let cursor_x = inner.x + app.cursor_pos as u16;
            if cursor_x < inner.x + inner.width {
                f.set_cursor(cursor_x, inner.y);
            }
        }
    }
}

fn draw_bottom_bar(f: &mut Frame, app: &App, area: Rect, panel_visible: bool, term_width: u16) {
    let hints = match app.mode {
        AppMode::Chat | AppMode::CommandMenu => {
            return f.render_widget(
                Paragraph::new(Line::from(if term_width >= 80 {
                    let panel_action = if panel_visible { "hide" } else { "show" };
                    let mut s = vec![
                        Span::styled(" / ", Style::default().fg(ACCENT)),
                        Span::styled("commands", Style::default().fg(DIM)),
                        Span::styled("  Ctrl+q ", Style::default().fg(ACCENT)),
                        Span::styled("quit", Style::default().fg(DIM)),
                        Span::styled("  Esc ", Style::default().fg(ACCENT)),
                        Span::styled("stop", Style::default().fg(DIM)),
                        Span::styled("  ↑↓ ", Style::default().fg(ACCENT)),
                        Span::styled("scroll", Style::default().fg(DIM)),
                        Span::styled("  Ctrl+d ", Style::default().fg(ACCENT)),
                        Span::styled(panel_action.to_string() + " panel", Style::default().fg(DIM)),
                    ];
                    if panel_visible && term_width >= 110 {
                        s.push(Span::styled("  Ctrl+← → ", Style::default().fg(ACCENT)));
                        s.push(Span::styled("resize", Style::default().fg(DIM)));
                    }
                    s
                } else {
                    vec![Span::styled(
                        " /  Ctrl+q  Esc  Ctrl+d panel",
                        Style::default().fg(DIM),
                    )]
                })).style(Style::default().bg(BG)),
                area,
            );
        }
        _ => "",
    };
    f.render_widget(
        Paragraph::new(Span::styled(hints, Style::default().fg(DIM))).style(Style::default().bg(BG)),
        area,
    );
}

#[allow(dead_code)]
fn draw_conversations(f: &mut Frame, app: &App, area: Rect) {
    let is_active = app.mode == AppMode::ConversationList;
    let border_color = if is_active { ACCENT } else { DIM };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            " Conversations ",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = if app.conversations.is_empty() {
        vec![ListItem::new(Span::styled("  no history", Style::default().fg(DIM)))]
    } else {
        app.conversations.iter().map(|c| {
            let label = if c.name.is_empty() { &c.preview } else { &c.name };
            let is_current = c.id == app.current_conversation_id;
            let color = if is_current { Color::White } else { Color::Gray };
            let prefix = if is_current { "> " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(ACCENT)),
                Span::styled(label.as_str(), Style::default().fg(color)),
            ]))
        }).collect()
    };

    let mut state = ListState::default();
    if is_active {
        state.select(Some(app.conversation_cursor));
    } else {
        // Highlight current conversation even when not in ConversationList mode
        let cur = app.conversations.iter().position(|c| c.id == app.current_conversation_id);
        state.select(cur);
    }

    let list = List::new(items)
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");

    f.render_stateful_widget(list, inner, &mut state);
}

#[allow(dead_code)]
fn draw_tasks(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(
            " Scheduled Tasks ",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));

    let items: Vec<ListItem> = if app.tasks.is_empty() {
        vec![ListItem::new(Span::styled("  no tasks", Style::default().fg(DIM)))]
    } else {
        app.tasks.iter().map(|t| {
            let color = if t.enabled { Color::Green } else { ERROR_COLOR };
            ListItem::new(Line::from(vec![
                Span::styled(if t.enabled { "* " } else { "o " }, Style::default().fg(color)),
                Span::styled(t.prompt.as_str(), Style::default().fg(Color::White)),
            ]))
        }).collect()
    };

    let mut state = ListState::default();
    if app.mode == AppMode::TaskManager {
        state.select(Some(app.task_cursor));
    }

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("> ");
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_model_overlay(f: &mut Frame, app: &App, area: Rect) {
    let popup_w = if area.width < 80 { 98 } else { 70 };
    let popup = centered_rect(popup_w, 88, area);
    f.render_widget(Clear, popup);

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(60, 100, 160)))
        .style(Style::default().bg(BG));
    f.render_widget(outer_block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(Rect {
            x: popup.x + 1, y: popup.y + 1,
            width: popup.width.saturating_sub(2),
            height: popup.height.saturating_sub(2),
        });

    let title_suffix = if app.hf_searching { " (searching online...)" } else { " (type to search)" };
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" Select Model{} ", title_suffix),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));
    let search_inner = search_block.inner(chunks[0]);
    f.render_widget(search_block, chunks[0]);

    let esc_rect = Rect {
        x: chunks[0].x + chunks[0].width.saturating_sub(9),
        y: chunks[0].y, width: 8, height: 1,
    };
    f.render_widget(
        Paragraph::new(Span::styled("esc ✕", Style::default().fg(DIM))),
        esc_rect,
    );

    let search_display = if app.model_search.is_empty() {
        Paragraph::new(Span::styled("Search models or providers...", Style::default().fg(Color::Rgb(40, 50, 70))))
    } else {
        Paragraph::new(Span::styled(app.model_search.as_str(), Style::default().fg(Color::White)))
    };
    f.render_widget(search_display, search_inner);
    f.set_cursor(search_inner.x + app.model_search.len() as u16, search_inner.y);

    let sections = build_model_list_grouped(app);
    let visible_height = chunks[1].height as usize;
    let cursor = app.model_cursor;

    struct DisplayRow {
        item: ListItem<'static>,
        selectable: bool,
        cursor_idx: usize,
    }

    let mut rows: Vec<DisplayRow> = Vec::new();
    let mut sel_i = 0usize;

    for section in &sections {
        rows.push(DisplayRow {
            item: ListItem::new(Line::from("")),
            selectable: false,
            cursor_idx: 0,
        });
        rows.push(DisplayRow {
            item: ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {} ", section.label),
                    Style::default()
                        .fg(section.header_color)
                        .bg(Color::Rgb(12, 18, 32))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {} ", section.count_label),
                    Style::default()
                        .fg(Color::Rgb(50, 65, 95))
                        .bg(Color::Rgb(12, 18, 32)),
                ),
            ])).style(Style::default().bg(Color::Rgb(12, 18, 32))),
            selectable: false,
            cursor_idx: 0,
        });

        for provider_group in &section.providers {
            if !provider_group.provider_label.is_empty() {
                let key_status = if provider_group.has_key {
                    Span::styled("  ✓ key saved", Style::default().fg(Color::Rgb(80, 200, 100)))
                } else {
                    Span::styled("  🔑 key needed", Style::default().fg(Color::Rgb(200, 160, 60)))
                };
                rows.push(DisplayRow {
                    item: ListItem::new(Line::from(vec![
                        Span::raw("   "),
                        Span::styled(
                            format!("▸ {}", provider_group.provider_label),
                            Style::default().fg(provider_group.provider_color).add_modifier(Modifier::BOLD),
                        ),
                        key_status,
                    ])),
                    selectable: false,
                    cursor_idx: 0,
                });
            }

            for entry in &provider_group.models {
                let is_active = entry.name == app.cfg.ai.model;
                let is_selected = sel_i == cursor;

                let mut spans = vec![
                    Span::styled(
                        if is_active { "    ▶ " } else { "      " },
                        Style::default().fg(ACCENT),
                    ),
                ];

                if !entry.is_installed && !entry.is_cloud {
                    spans.push(Span::styled("⬇ ", Style::default().fg(Color::Rgb(80, 160, 255))));
                } else if entry.is_cloud {
                    spans.push(Span::styled("☁ ", Style::default().fg(Color::Rgb(120, 80, 200))));
                } else {
                    spans.push(Span::raw("  "));
                }

                let name_color = if is_active {
                    Color::White
                } else if entry.is_installed || entry.is_cloud {
                    Color::Rgb(190, 200, 220)
                } else {
                    Color::Rgb(90, 105, 130)
                };

                spans.push(Span::styled(
                    entry.name.clone(),
                    Style::default()
                        .fg(name_color)
                        .add_modifier(if is_active { Modifier::BOLD } else { Modifier::empty() }),
                ));

                if entry.size_gb > 0.0 {
                    spans.push(Span::styled(
                        format!("  {:.1}GB", entry.size_gb),
                        Style::default().fg(Color::Rgb(50, 65, 90)),
                    ));
                }
                if entry.vision {
                    spans.push(Span::styled("  👁", Style::default().fg(Color::Rgb(70, 170, 90))));
                }

                let row_style = if is_selected {
                    Style::default().bg(Color::Rgb(25, 35, 58))
                } else {
                    Style::default()
                };

                rows.push(DisplayRow {
                    item: ListItem::new(Line::from(spans)).style(row_style),
                    selectable: true,
                    cursor_idx: sel_i,
                });
                sel_i += 1;
            }
        }

        rows.push(DisplayRow {
            item: ListItem::new(Line::from("")),
            selectable: false,
            cursor_idx: 0,
        });
    }

    let selected_display_idx = rows.iter()
        .position(|r| r.selectable && r.cursor_idx == cursor)
        .unwrap_or(0);

    let scroll_start = if selected_display_idx >= visible_height {
        selected_display_idx.saturating_sub(visible_height / 2)
    } else { 0 }.min(rows.len().saturating_sub(visible_height));

    let items: Vec<ListItem> = rows.iter().map(|r| r.item.clone()).collect();

    let list_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::Rgb(20, 30, 50)))
        .style(Style::default().bg(BG));

    let mut state = ListState::default();
    state.select(Some(selected_display_idx));
    *state.offset_mut() = scroll_start;

    f.render_stateful_widget(
        List::new(items)
            .block(list_block)
            .highlight_style(
                Style::default()
                    .bg(Color::Rgb(25, 35, 58))
                    .add_modifier(Modifier::BOLD),
            ),
        chunks[1],
        &mut state,
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ↑↓ ", Style::default().fg(ACCENT)),
            Span::styled("navigate  ", Style::default().fg(DIM)),
            Span::styled("enter ", Style::default().fg(ACCENT)),
            Span::styled("select  ", Style::default().fg(DIM)),
            Span::styled("⬇ ", Style::default().fg(Color::Rgb(80, 160, 255))),
            Span::styled("download  ", Style::default().fg(DIM)),
            Span::styled("type ", Style::default().fg(ACCENT)),
            Span::styled("search", Style::default().fg(DIM)),
        ]))
        .style(Style::default().bg(BG)),
        chunks[2],
    );
}

fn draw_api_key_overlay(f: &mut Frame, app: &App, area: Rect) {
    // Responsive: use fixed height based on content, capped to available space
    let needed_height = 16u16;
    let popup_height_pct = ((needed_height as f32 / area.height as f32) * 100.0) as u16;
    let popup_height_pct = popup_height_pct.clamp(50, 90);
    let popup_width_pct = if area.width < 80 { 95 } else if area.width < 120 { 75 } else { 60 };
    let popup = centered_rect(popup_width_pct, popup_height_pct, area);
    f.render_widget(Clear, popup);

    let is_validating = app.status.starts_with("Validating");
    let has_error = app.status.starts_with("Key rejected");

    let border_color = if is_validating { Color::Yellow }
                       else if has_error { Color::Rgb(220, 80, 80) }
                       else { ACCENT };

    let title = if is_validating { " Verifying... " }
                else if has_error { " ✗ Invalid Key — Try Again " }
                else { " 🔑 API Key Required " };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(title,
            Style::default().fg(border_color).add_modifier(Modifier::BOLD)))
        .style(Style::default().bg(BG));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    // Guard against tiny terminals
    if inner.height < 10 {
        f.render_widget(
            Paragraph::new("Terminal too small - resize to continue")
                .style(Style::default().fg(Color::Yellow)),
            inner,
        );
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // 0: provider + model
            Constraint::Length(1), // 1: spacer
            Constraint::Length(1), // 2: get key link
            Constraint::Length(1), // 3: spacer
            Constraint::Length(3), // 4: input box
            Constraint::Length(1), // 5: spacer
            Constraint::Length(1), // 6: status/hint
            Constraint::Length(1), // 7: spacer
            Constraint::Length(1), // 8: submit hint
            Constraint::Min(0),
        ])
        .split(inner);

    // Row 0: provider › model
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Provider  ", Style::default().fg(DIM)),
            Span::styled(app.pending_provider.clone(),
                Style::default().fg(Color::Rgb(160, 100, 255)).add_modifier(Modifier::BOLD)),
            Span::styled("  ›  ", Style::default().fg(DIM)),
            Span::styled(app.pending_model.clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ])),
        rows[0],
    );

    // Row 2: where to get the key
    let url = match app.pending_provider.as_str() {
        "openai"     => "platform.openai.com/api-keys",
        "anthropic"  => "console.anthropic.com/keys",
        "gemini"     => "aistudio.google.com/apikey",
        "mistral"    => "console.mistral.ai/api-keys",
        "groq"       => "console.groq.com/keys",
        "openrouter" => "openrouter.ai/keys",
        "nvidia"     => "build.nvidia.com",
        "together"   => "api.together.xyz/settings/api-keys",
        _            => "",
    };
    if !url.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Get key → ", Style::default().fg(DIM)),
                Span::styled(url, Style::default().fg(Color::Rgb(80, 140, 255)).add_modifier(Modifier::UNDERLINED)),
            ])),
            rows[2],
        );
    }

    // Row 4: input box with masking
    let raw_len = app.api_key_input.len();
    let display_key = if raw_len == 0 {
        "".to_string()
    } else {
        let visible = &app.api_key_input[raw_len.saturating_sub(4)..];
        format!("{}{}", "•".repeat(raw_len.saturating_sub(4)), visible)
    };
    f.render_widget(
        Paragraph::new(Span::styled(
            if raw_len == 0 { "sk-... paste here".to_string() } else { display_key },
            Style::default().fg(if raw_len == 0 { Color::Rgb(50, 55, 80) } else { Color::Rgb(120, 220, 140) }),
        )).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(if raw_len > 0 { Color::Rgb(80, 200, 130) } else { DIM }))
                .style(Style::default().bg(Color::Rgb(10, 16, 30)))
        ),
        rows[4],
    );
    f.set_cursor(
        rows[4].x + 1 + (raw_len as u16).min(rows[4].width.saturating_sub(3)),
        rows[4].y + 1,
    );

    // Row 6: status
    let elapsed = if is_validating {
        app.thinking_start.map(|s| s.elapsed().as_secs()).unwrap_or(0)
    } else { 0 };

    let (status_text, status_color) = if is_validating {
        (format!("⏳ Verifying... ({}s)", elapsed), Color::Yellow)
    } else if has_error {
        let msg = app.status.trim_start_matches("Key rejected: ");
        (format!("✗  {}", msg), Color::Rgb(255, 90, 90))
    } else if raw_len > 0 {
        (format!("✓  {} chars - looks good", raw_len), Color::Rgb(80, 200, 130))
    } else {
        ("".to_string(), Color::Rgb(80, 90, 120))
    };
    f.render_widget(
        Paragraph::new(Span::styled(status_text, Style::default().fg(status_color))),
        rows[6],
    );

    // Row 8: submit/cancel hint
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Enter ", Style::default().fg(Color::Rgb(12,14,20)).bg(Color::Rgb(80, 200, 130)).add_modifier(Modifier::BOLD)),
            Span::styled(" confirm  ", Style::default().fg(DIM)),
            Span::styled(" Esc ", Style::default().fg(Color::Rgb(12,14,20)).bg(Color::Rgb(60, 65, 90)).add_modifier(Modifier::BOLD)),
            Span::styled(" cancel", Style::default().fg(DIM)),
        ])),
        rows[8],
    );
}

fn draw_key_manager(f: &mut Frame, app: &App, area: Rect) {
    use crate::tui::model_fetch::PROVIDERS;

    let popup_w = if area.width < 80 { 95 } else { 65 };
    let popup = centered_rect(popup_w, 85, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " API Key Manager — Tab: switch provider ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let provider_names: Vec<String> = PROVIDERS.iter().map(|p| {
        let count = app.provider_keys_multi.get(p.id).map(|v| v.len()).unwrap_or(0);
        format!(" {} ({}) ", p.name, count)
    }).collect();

    let tab_spans: Vec<Span> = PROVIDERS.iter().zip(provider_names.iter()).map(|(p, name)| {
        if p.id == app.key_manager_provider.as_str() {
            Span::styled(name.clone(), Style::default()
                .fg(Color::Black).bg(ACCENT).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(name.clone(), Style::default().fg(Color::White))
        }
    }).collect();
    f.render_widget(Paragraph::new(Line::from(tab_spans)), chunks[0]);

    let empty = vec![];
    let keys = app.provider_keys_multi.get(&app.key_manager_provider).unwrap_or(&empty);
    let active_idx = app.provider_key_index.get(&app.key_manager_provider).copied().unwrap_or(0);

    let items: Vec<ListItem> = if keys.is_empty() {
        vec![ListItem::new(Span::styled(
            "  No keys saved. Press 'a' to add one.",
            Style::default().fg(DIM),
        ))]
    } else {
        keys.iter().enumerate().map(|(i, k)| {
            let is_sel = i == app.key_manager_cursor;
            let is_active = i == active_idx;
            let masked = if k.len() > 8 {
                format!("{}...{}", &k[..4], &k[k.len()-4..])
            } else {
                "••••••••".to_string()
            };
            let status = if is_active { " ◀ active" } else { "" };
            ListItem::new(Line::from(vec![
                Span::styled(
                    if is_sel { "  ▶ " } else { "    " },
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    format!("Key {}: {}{}", i + 1, masked, status),
                    Style::default()
                        .fg(if is_active { Color::Green } else { Color::White })
                        .add_modifier(if is_sel { Modifier::BOLD } else { Modifier::empty() }),
                ),
            ]))
        }).collect()
    };

    let mut state = ListState::default();
    state.select(Some(app.key_manager_cursor.min(keys.len().saturating_sub(1))));
    f.render_stateful_widget(
        List::new(items)
            .block(Block::default()
                .borders(Borders::TOP | Borders::BOTTOM)
                .border_style(Style::default().fg(DIM)))
            .highlight_style(Style::default().bg(Color::Rgb(25, 35, 55))),
        chunks[2],
        &mut state,
    );

    if app.key_manager_adding {
        let raw_len = app.key_manager_input.len();
        let display = if raw_len == 0 {
            "paste key here...".to_string()
        } else {
            format!("{}...{}", &app.key_manager_input[..raw_len.min(4)],
                &app.key_manager_input[raw_len.saturating_sub(4)..])
        };
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" New key: ", Style::default().fg(DIM)),
                Span::styled(display, Style::default().fg(Color::Green)),
                Span::styled(" (Enter confirm, Esc cancel)", Style::default().fg(DIM)),
            ])),
            chunks[3],
        );
    } else {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  Keys rotate automatically on rate limit",
                Style::default().fg(DIM),
            )),
            chunks[3],
        );
    }

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Tab", Style::default().fg(ACCENT)),
            Span::styled(" provider  ", Style::default().fg(DIM)),
            Span::styled("a", Style::default().fg(ACCENT)),
            Span::styled(" add  ", Style::default().fg(DIM)),
            Span::styled("d", Style::default().fg(Color::Red)),
            Span::styled(" delete  ", Style::default().fg(DIM)),
            Span::styled("↑↓", Style::default().fg(ACCENT)),
            Span::styled(" select", Style::default().fg(DIM)),
        ])),
        chunks[4],
    );
}

fn draw_download_confirm(f: &mut Frame, app: &App, area: Rect) {
    use super::app::OLLAMA_CATALOG;
    let needed = 12u16;
    let h_pct = ((needed as f32 / area.height as f32) * 100.0) as u16;
    let h_pct = h_pct.clamp(30, 60);
    let popup_w = if area.width < 80 { 95 } else { 50 };
    let popup = centered_rect(popup_w, h_pct, area);
    f.render_widget(Clear, popup);

    let info = OLLAMA_CATALOG.iter().find(|k| k.name == app.pending_download_model.as_str());
    let size_str = info.map(|k| format!("{:.1} GB", k.size_gb)).unwrap_or_else(|| "unknown size".to_string());
    let vision_str = info.map(|k| if k.vision { "  👁 vision capable" } else { "" }).unwrap_or("");

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(80, 160, 255)))
        .title(Span::styled(
            " Download Model ",
            Style::default().fg(Color::Rgb(80, 160, 255)).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Model:  ", Style::default().fg(DIM)),
            Span::styled(
                app.pending_download_model.clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Size:   ", Style::default().fg(DIM)),
            Span::styled(size_str, Style::default().fg(Color::Rgb(80, 200, 120))),
            Span::styled(vision_str, Style::default().fg(Color::Rgb(80, 180, 100))),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Ollama will pull this model in the background.",
            Style::default().fg(Color::Rgb(140, 140, 160)),
        )),
        Line::from(Span::styled(
            "  You can keep chatting while it downloads.",
            Style::default().fg(Color::Rgb(140, 140, 160)),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(" y / Enter ", Style::default().fg(Color::Rgb(12, 14, 20)).bg(Color::Rgb(80, 160, 255)).add_modifier(Modifier::BOLD)),
            Span::styled("  confirm download", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(" n / Esc   ", Style::default().fg(DIM).bg(Color::Rgb(30, 30, 40))),
            Span::styled("  cancel", Style::default().fg(DIM)),
        ]),
    ];

    f.render_widget(
        Paragraph::new(text).style(Style::default().bg(BG)),
        inner,
    );
}

fn draw_conversation_list(f: &mut Frame, app: &App, area: Rect) {
    let popup_w = if area.width < 80 { 98 } else { 62 };
    let popup = centered_rect(popup_w, 88, area);
    f.render_widget(Clear, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0), Constraint::Length(1)])
        .split(popup);

    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(" Sessions ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)))
        .title_alignment(Alignment::Left);
    let search_inner = search_block.inner(chunks[0]);
    f.render_widget(search_block, chunks[0]);

    // Esc hint top-right corner
    let esc_rect = Rect {
        x: chunks[0].x + chunks[0].width.saturating_sub(9),
        y: chunks[0].y,
        width: 8,
        height: 1,
    };
    f.render_widget(
        Paragraph::new(Span::styled("Esc ✕ ", Style::default().fg(DIM))),
        esc_rect,
    );

    let search_display = if app.conv_search.is_empty() {
        Paragraph::new(Span::styled("Search...", Style::default().fg(DIM)))
    } else {
        Paragraph::new(app.conv_search.as_str())
    };
    f.render_widget(search_display, search_inner);
    if !app.conv_search.is_empty() {
        f.set_cursor(search_inner.x + app.conv_search.len() as u16, search_inner.y);
    } else {
        f.set_cursor(search_inner.x, search_inner.y);
    }

    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 60)));

    let convs = filtered_convs(&app.conversations, &app.conv_search);
    let items: Vec<ListItem> = if convs.is_empty() {
        vec![ListItem::new(Span::styled("  no conversations found", Style::default().fg(DIM)))]
    } else {
        convs.iter().map(|c| {
            let label = if c.name.is_empty() { &c.preview } else { &c.name };
            let is_current = c.id == app.current_conversation_id;
            ListItem::new(Line::from(vec![
                Span::styled(if is_current { "> " } else { "  " }, Style::default().fg(ACCENT)),
                Span::styled(label.as_str(), Style::default().fg(if is_current { Color::White } else { Color::Gray })),
            ]))
        }).collect()
    };

    let mut state = ListState::default();
    state.select(Some(app.conversation_cursor.min(convs.len().saturating_sub(1))));

    f.render_stateful_widget(
        List::new(items).block(list_block)
            .highlight_style(Style::default().bg(Color::Rgb(30, 40, 55)).add_modifier(Modifier::BOLD))
            .highlight_symbol("> "),
        chunks[1],
        &mut state,
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" enter", Style::default().fg(ACCENT)),
            Span::styled(" load  ", Style::default().fg(DIM)),
            Span::styled("ctrl+r", Style::default().fg(ACCENT)),
            Span::styled(" rename  ", Style::default().fg(DIM)),
            Span::styled("ctrl+d", Style::default().fg(ACCENT)),
            Span::styled(" delete ", Style::default().fg(DIM)),
        ])),
        chunks[2],
    );
}

fn draw_rename_overlay(f: &mut Frame, app: &App, area: Rect) {
    let needed = 5u16;
    let h_pct = ((needed as f32 / area.height as f32) * 100.0) as u16;
    let h_pct = h_pct.clamp(15, 35);
    let popup_w = if area.width < 80 { 95 } else { 50 };
    let popup = centered_rect(popup_w, h_pct, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(" Rename - Enter save - Esc cancel ", Style::default().fg(ACCENT)));

    let inner = block.inner(popup);
    f.render_widget(block, popup);
    f.render_widget(Paragraph::new(app.rename_input.as_str()), inner);
    f.set_cursor(inner.x + app.rename_input.len() as u16, inner.y);
}

fn draw_delete_confirm(f: &mut Frame, app: &App, area: Rect) {
    let needed = 10u16;
    let h_pct = ((needed as f32 / area.height as f32) * 100.0) as u16;
    let h_pct = h_pct.clamp(25, 50);
    let popup_w = if area.width < 80 { 95 } else { 46 };
    let popup = centered_rect(popup_w, h_pct, area);
    f.render_widget(Clear, popup);

    let name = app.conversations.get(app.conversation_cursor)
        .map(|c| if c.name.is_empty() { c.preview.as_str() } else { c.name.as_str() })
        .unwrap_or("this conversation");

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ERROR_COLOR))
        .title(Span::styled(" Delete Conversation ", Style::default().fg(ERROR_COLOR).add_modifier(Modifier::BOLD)));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  \"{}\"", name),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled("  This cannot be undone.", Style::default().fg(DIM))),
        Line::from(""),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Enter / y  ", Style::default().fg(Color::Rgb(12,14,20)).bg(ERROR_COLOR).add_modifier(Modifier::BOLD)),
            Span::styled("  confirm delete", Style::default().fg(ERROR_COLOR)),
        ]),
        Line::from(vec![
            Span::styled("  Esc / n    ", Style::default().fg(DIM).bg(Color::Rgb(25,28,40)).add_modifier(Modifier::BOLD)),
            Span::styled("  cancel", Style::default().fg(DIM)),
        ]),
    ];
    f.render_widget(Paragraph::new(text), inner);
}

fn draw_help_overlay(f: &mut Frame, area: Rect) {
    let popup_w = if area.width < 80 { 98 } else { 55 };
    let popup = centered_rect(popup_w, 92, area);
    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(" Help - Esc to close ", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)));

    let text = vec![
        Line::from(""),
        Line::from(Span::styled("  Commands (type /)", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  /new            New conversation", Style::default().fg(Color::White))),
        Line::from(Span::styled("  /conversations  Browse all sessions", Style::default().fg(Color::White))),
        Line::from(Span::styled("  /model          Switch AI model", Style::default().fg(Color::White))),
        Line::from(Span::styled("  /tasks          Manage scheduled tasks", Style::default().fg(Color::White))),
        Line::from(Span::styled("  /clear          Clear current chat", Style::default().fg(Color::White))),
        Line::from(Span::styled("  /info           System info", Style::default().fg(Color::White))),
        Line::from(Span::styled("  /quit           Quit (confirm twice)", Style::default().fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled("  Keyboard", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  Enter       Send message", Style::default().fg(Color::White))),
        Line::from(Span::styled("  up/down     Scroll chat", Style::default().fg(Color::White))),
        Line::from(Span::styled("  Ctrl+E      Expand/collapse thinking", Style::default().fg(Color::White))),
        Line::from(Span::styled("  Esc         Stop current task", Style::default().fg(Color::White))),
        Line::from(Span::styled("  Ctrl+Q      Quit (confirm)", Style::default().fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled("  Conversations", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  Enter       Load selected", Style::default().fg(Color::White))),
        Line::from(Span::styled("  Ctrl+R      Rename", Style::default().fg(Color::White))),
        Line::from(Span::styled("  Ctrl+D      Delete (with confirm)", Style::default().fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled("  Top bar", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("  ctx shows tokens used/limit (%)", Style::default().fg(Color::White))),
        Line::from(Span::styled("  cost shown for cloud providers", Style::default().fg(Color::White))),
    ];

    f.render_widget(Paragraph::new(text).block(block), popup);
}

fn draw_command_menu(f: &mut Frame, app: &App, area: Rect) {
    let filtered = filtered_commands(&app.input);
    let menu_height = (filtered.len() as u16 + 2).max(3);

    let menu_rect = Rect {
        x: area.x + 2,
        y: area.height.saturating_sub(menu_height + 4),
        width: 48,
        height: menu_height,
    };
    f.render_widget(Clear, menu_rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" Commands ({}) - Esc cancel ", filtered.len()),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    let items: Vec<ListItem> = if filtered.is_empty() {
        vec![ListItem::new(Span::styled("  no match", Style::default().fg(DIM)))]
    } else {
        filtered.iter().map(|(cmd, desc)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:<20}", cmd), Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(desc.to_string(), Style::default().fg(Color::Gray)),
            ]))
        }).collect()
    };

    let mut state = ListState::default();
    state.select(Some(app.command_menu_cursor.min(filtered.len().saturating_sub(1))));

    f.render_stateful_widget(
        List::new(items).block(block)
            .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
            .highlight_symbol("> "),
        menu_rect,
        &mut state,
    );
}

fn draw_device_panel(f: &mut Frame, app: &App, area: Rect, scroll: usize) {
    f.render_widget(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(Color::Rgb(18, 28, 48)))
            .style(Style::default().bg(Color::Rgb(5, 10, 20))),
        area,
    );

    let inner = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(1),
        height: area.height,
    };

    let val_w = inner.width.saturating_sub(8) as usize;

    let wrap_val = |s: &str| -> Vec<String> {
        if s.is_empty() { return vec!["—".to_string()]; }
        let mut out = Vec::new();
        let mut cur = String::new();
        for word in s.split_whitespace() {
            if cur.is_empty() {
                cur = word.to_string();
            } else if cur.len() + 1 + word.len() <= val_w {
                cur.push(' ');
                cur.push_str(word);
            } else {
                out.push(cur.clone());
                cur = word.to_string();
            }
        }
        if !cur.is_empty() { out.push(cur); }
        if out.is_empty() { out.push("—".to_string()); }
        out
    };

    let short_val = |s: &str| -> String {
        if s.is_empty() { "—".to_string() } else { s.to_string() }
    };

    let key_style = Style::default().fg(Color::Rgb(50, 68, 100));
    let val_style = Style::default().fg(Color::Rgb(190, 210, 240));
    let cont_style = Style::default().fg(Color::Rgb(140, 160, 200));
    let sec_style  = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
    let div_style  = Style::default().fg(Color::Rgb(18, 28, 48));

    let mut lines: Vec<Line> = Vec::new();

    let mut add_row = |lines: &mut Vec<Line>, key: &str, val: &str| {
        let parts = wrap_val(val);
        let first = parts.first().cloned().unwrap_or_default();
        lines.push(Line::from(vec![
            Span::styled(format!("  {:<5} ", key), key_style),
            Span::styled(first, val_style),
        ]));
        for cont in parts.iter().skip(1) {
            lines.push(Line::from(vec![
                Span::styled("        ".to_string(), key_style),
                Span::styled(cont.clone(), cont_style),
            ]));
        }
    };

    let add_section = |lines: &mut Vec<Line>, title: &str| {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ".to_string(), div_style),
            Span::styled(title.to_string(), sec_style),
        ]));
        lines.push(Line::from(
            Span::styled(
                format!("  {}", "─".repeat(inner.width.saturating_sub(3) as usize)),
                div_style,
            )
        ));
    };

    add_section(&mut lines, "System");
    add_row(&mut lines, "host", &short_val(&app.device_hostname));
    add_row(&mut lines, "os",   &short_val(&app.device_os_edition));
    add_row(&mut lines, "tz",   &short_val(&app.device_timezone));

    add_section(&mut lines, "Hardware");
    add_row(&mut lines, "cpu",   &short_val(&app.device_cpu_model));
    add_row(&mut lines, "arch",  &short_val(&app.device_cpu_arch));
    let cores_str = if app.device_cpu_freq_mhz > 0 {
        format!("{}c {}lc  {:.2}GHz",
            app.device_cpu_cores,
            app.device_cpu_logical,
            app.device_cpu_freq_mhz as f64 / 1000.0)
    } else {
        format!("{}c {}lc", app.device_cpu_cores, app.device_cpu_logical)
    };
    add_row(&mut lines, "cores", &cores_str);
    add_row(&mut lines, "ram",   &format!("{:.1}GB", app.device_ram_mb as f64 / 1024.0));
    if app.device_gpus.len() <= 1 {
        add_row(&mut lines, "gpu", &short_val(&app.device_gpu));
    } else {
        for (i, g) in app.device_gpus.iter().enumerate() {
            let label = if i == 0 { "gpu ★".to_string() } else { format!("gpu{}", i + 1) };
            add_row(&mut lines, &label, g);
        }
    }

    add_section(&mut lines, "Display");
    if app.device_screens.len() <= 1 {
        add_row(&mut lines, "res",   &format!("{}x{}", app.device_screen_w, app.device_screen_h));
        add_row(&mut lines, "scale", &format!("{:.1}x", app.device_screen_scale));
        add_row(&mut lines, "touch", if app.device_touch { "yes" } else { "no" });
    } else {
        for (i, s) in app.device_screens.iter().enumerate() {
            let label = if s.is_primary {
                format!("mon{} ★", i + 1)
            } else {
                format!("mon{}", i + 1)
            };
            add_row(&mut lines, &label, &format!("{}x{} {:.1}x", s.width, s.height, s.scale_factor));
        }
        add_row(&mut lines, "touch", if app.device_touch { "yes" } else { "no" });
    }

    let total = lines.len();
    let visible = inner.height as usize;
    let max_scroll = total.saturating_sub(visible);
    let scroll = scroll.min(max_scroll);

    f.render_widget(
        Paragraph::new(lines)
            .scroll((scroll as u16, 0))
            .style(Style::default().bg(Color::Rgb(5, 10, 20))),
        inner,
    );
}

fn draw_clarification(f: &mut Frame, app: &App, area: Rect) {
    let popup = centered_rect(60, 70, area);
    f.render_widget(Clear, popup);

    let question = app.clarification_question.as_deref().unwrap_or("What would you like to do?");

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Clarification needed ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(Span::styled(question, Style::default().fg(Color::White)))
            .wrap(Wrap { trim: false }),
        chunks[0],
    );

    let items: Vec<ListItem> = app.clarification_options.iter().enumerate().map(|(i, opt)| {
        let selected = i == app.clarification_cursor;
        ListItem::new(Line::from(vec![
            Span::styled(
                if selected { "  ▶ " } else { "    " },
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                opt.clone(),
                Style::default()
                    .fg(if selected { Color::White } else { Color::Rgb(160, 170, 190) })
                    .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
            ),
        ]))
    }).collect();

    let mut state = ListState::default();
    state.select(Some(app.clarification_cursor));

    f.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().bg(Color::Rgb(25, 35, 58))),
        chunks[1],
        &mut state,
    );

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ↑↓ ", Style::default().fg(ACCENT)),
            Span::styled("select  ", Style::default().fg(DIM)),
            Span::styled("Enter ", Style::default().fg(ACCENT)),
            Span::styled("confirm  ", Style::default().fg(DIM)),
            Span::styled("Esc ", Style::default().fg(ACCENT)),
            Span::styled("cancel", Style::default().fg(DIM)),
        ])),
        chunks[2],
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn draw_onboarding(
    f: &mut Frame,
    ob: &crate::tui::onboarding::OnboardingState,
    area: Rect,
) {
    use crate::tui::onboarding::{
        OnboardingStep, ProviderFocus, KeyStatus, ONBOARDING_PROVIDERS,
    };

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BG))
        .title(Span::styled(
            " ⚡ Antflow — First-time Setup ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(outer, area);

    let inner = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };

    let steps = ["Welcome", "Storage", "Provider", "Memory", "Done"];
    let current_idx = match ob.step {
        OnboardingStep::Welcome  => 0,
        OnboardingStep::Storage  => 1,
        OnboardingStep::Provider => 2,
        OnboardingStep::Memory   => 3,
        OnboardingStep::Done     => 4,
    };
    let step_spans: Vec<Span> = steps.iter().enumerate().map(|(i, name)| {
        if i == current_idx {
            Span::styled(
                format!(" ● {} ", name),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )
        } else if i < current_idx {
            Span::styled(format!(" ✓ {} ", name), Style::default().fg(Color::Rgb(60, 140, 80)))
        } else {
            Span::styled(format!(" ○ {} ", name), Style::default().fg(DIM))
        }
    }).collect();
    f.render_widget(
        Paragraph::new(Line::from(step_spans)).alignment(Alignment::Center),
        Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 },
    );

    let body = Rect {
        x: inner.x,
        y: inner.y + 2,
        width: inner.width,
        height: inner.height.saturating_sub(4),
    };
    let footer = Rect {
        x: inner.x,
        y: inner.y + inner.height.saturating_sub(2),
        width: inner.width,
        height: 1,
    };

    match ob.step {
        OnboardingStep::Welcome  => draw_ob_welcome(f, body),
        OnboardingStep::Storage  => draw_ob_storage(f, ob, body),
        OnboardingStep::Provider => draw_ob_provider(f, ob, body),
        OnboardingStep::Memory   => draw_ob_memory(f, ob, body),
        OnboardingStep::Done     => draw_ob_done(f, ob, body),
    }

    if !ob.status_line.is_empty() {
        let is_err = ob.status_line.starts_with('✗');
        let color = if is_err { Color::Rgb(220, 80, 80) } else { Color::Rgb(80, 200, 120) };
        f.render_widget(
            Paragraph::new(Span::styled(ob.status_line.clone(), Style::default().fg(color)))
                .alignment(Alignment::Center),
            Rect { x: inner.x, y: footer.y.saturating_sub(1), width: inner.width, height: 1 },
        );
    }

    let hints = match ob.step {
        OnboardingStep::Welcome => " Enter  start setup   F1  skip (auto mode) ",
        OnboardingStep::Storage => " ↑↓  select   Enter  confirm   Esc  back ",
        OnboardingStep::Provider => " ↑↓  navigate   Tab  switch panel   Enter  confirm   Esc  back ",
        OnboardingStep::Memory => " Space  toggle   Enter  next   Esc  back ",
        OnboardingStep::Done => " Enter  launch Antflow ",
    };
    f.render_widget(
        Paragraph::new(Span::styled(hints, Style::default().fg(DIM)))
            .alignment(Alignment::Center),
        footer,
    );
}

fn draw_ob_welcome(f: &mut Frame, area: Rect) {
    let cy = area.y + area.height / 2;
    let lines = vec![
        Line::from(Span::styled(
            "⚡  Welcome to Antflow",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "AI-powered computer automation — local or cloud, your choice.",
            Style::default().fg(Color::Rgb(180, 190, 210)),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "This wizard takes about 30 seconds.",
            Style::default().fg(DIM),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  Press Enter to begin  ",
                Style::default()
                    .fg(Color::Rgb(10, 14, 28))
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("   or   ", Style::default().fg(DIM)),
            Span::styled(
                "  F1 to skip (auto-detect)  ",
                Style::default().fg(DIM).bg(Color::Rgb(25, 30, 50)),
            ),
        ]),
    ];
    let total = lines.len() as u16;
    let start_y = cy.saturating_sub(total / 2);
    for (i, line) in lines.into_iter().enumerate() {
        if start_y + i as u16 >= area.y + area.height { break; }
        f.render_widget(
            Paragraph::new(line).alignment(Alignment::Center),
            Rect { x: area.x, y: start_y + i as u16, width: area.width, height: 1 },
        );
    }
}

fn draw_ob_storage(
    f: &mut Frame,
    ob: &crate::tui::onboarding::OnboardingState,
    area: Rect,
) {
    let popup = centered_rect(60, 70, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(60, 100, 160)))
        .title(Span::styled(
            " Step 1 — Storage location ",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new(Span::styled(
            "Where should Antflow store its data?",
            Style::default().fg(Color::Rgb(180, 190, 210)),
        )),
        chunks[0],
    );

    let options = [
        ("Portable", "Store everything next to the binary — ideal for USB drives"),
        ("Global",   "Store in ~/.config/antflow  (or AppData on Windows)"),
    ];
    let items: Vec<ListItem> = options.iter().enumerate().map(|(i, (name, desc))| {
        let selected = i == ob.storage_cursor;
        ListItem::new(vec![
            Line::from(vec![
                Span::styled(
                    if selected { "  ▶ " } else { "    " },
                    Style::default().fg(ACCENT),
                ),
                Span::styled(
                    *name,
                    Style::default()
                        .fg(if selected { Color::White } else { Color::Rgb(150, 160, 180) })
                        .add_modifier(if selected { Modifier::BOLD } else { Modifier::empty() }),
                ),
            ]),
            Line::from(Span::styled(
                format!("        {}", desc),
                Style::default().fg(DIM),
            )),
            Line::from(""),
        ])
    }).collect();

    let mut state = ListState::default();
    state.select(Some(ob.storage_cursor));
    f.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().bg(Color::Rgb(20, 30, 50))),
        chunks[2],
        &mut state,
    );
}

fn draw_ob_provider(
    f: &mut Frame,
    ob: &crate::tui::onboarding::OnboardingState,
    area: Rect,
) {
    use crate::tui::onboarding::{ProviderFocus, KeyStatus, ONBOARDING_PROVIDERS};

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(26),
            Constraint::Min(0),
            Constraint::Length(36),
        ])
        .split(area);

    let provider_border = if ob.provider_focus == ProviderFocus::ProviderList {
        ACCENT
    } else {
        DIM
    };
    let provider_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(provider_border))
        .title(Span::styled(
            " Step 2 — Provider ",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));
    let provider_inner = provider_block.inner(cols[0]);
    f.render_widget(provider_block, cols[0]);

    let provider_items: Vec<ListItem> = ONBOARDING_PROVIDERS.iter().enumerate().map(|(i, p)| {
        let sel = i == ob.provider_cursor;
        ListItem::new(Line::from(vec![
            Span::styled(if sel { " ▶ " } else { "   " }, Style::default().fg(p.color)),
            Span::styled(
                p.label,
                Style::default()
                    .fg(if sel { Color::White } else { Color::Rgb(160, 170, 190) })
                    .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
            ),
        ]))
    }).collect();

    let mut pstate = ListState::default();
    pstate.select(Some(ob.provider_cursor));
    f.render_stateful_widget(
        List::new(provider_items)
            .highlight_style(Style::default().bg(Color::Rgb(20, 28, 48))),
        provider_inner,
        &mut pstate,
    );

    let is_ollama = ob.current_provider().map(|p| p.id == "ollama").unwrap_or(false);
    let model_border = if matches!(
        ob.provider_focus,
        ProviderFocus::ModelList | ProviderFocus::OllamaModelList
    ) {
        ACCENT
    } else {
        DIM
    };

    let model_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(model_border))
        .title(Span::styled(
            " Model ",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));
    let model_inner = model_block.inner(cols[1]);
    f.render_widget(model_block, cols[1]);

    if is_ollama {
        if ob.ollama_models.is_empty() {
            let msg = if ob.ollama_fetched {
                "  No models installed.\n  Run: ollama pull llava"
            } else {
                "  Checking Ollama..."
            };
            f.render_widget(
                Paragraph::new(Span::styled(msg, Style::default().fg(DIM))),
                model_inner,
            );
        } else {
            let items: Vec<ListItem> = ob.ollama_models.iter().enumerate().map(|(i, m)| {
                let sel = i == ob.model_cursor
                    && ob.provider_focus == ProviderFocus::OllamaModelList;
                let vision = crate::ai::ollama::is_vision_model(m);
                ListItem::new(Line::from(vec![
                    Span::styled(if sel { " ▶ " } else { "   " }, Style::default().fg(ACCENT)),
                    Span::styled(
                        m.clone(),
                        Style::default().fg(if sel { Color::White } else { Color::Rgb(160, 170, 190) }),
                    ),
                    Span::styled(
                        if vision { " 👁" } else { "" },
                        Style::default().fg(Color::Rgb(70, 170, 90)),
                    ),
                ]))
            }).collect();
            let mut mstate = ListState::default();
            if ob.provider_focus == ProviderFocus::OllamaModelList {
                mstate.select(Some(ob.model_cursor));
            }
            f.render_stateful_widget(
                List::new(items)
                    .highlight_style(Style::default().bg(Color::Rgb(20, 28, 48))),
                model_inner,
                &mut mstate,
            );
        }
    } else {
        let models = ob.models_for_current_provider();
        let items: Vec<ListItem> = models.iter().enumerate().map(|(i, m)| {
            let sel = i == ob.model_cursor
                && ob.provider_focus == ProviderFocus::ModelList;
            ListItem::new(Line::from(vec![
                Span::styled(if sel { " ▶ " } else { "   " }, Style::default().fg(ACCENT)),
                Span::styled(
                    m.model_id,
                    Style::default()
                        .fg(if sel { Color::White } else { Color::Rgb(160, 170, 190) })
                        .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
                ),
                Span::styled(
                    if m.vision { " 👁" } else { "" },
                    Style::default().fg(Color::Rgb(70, 170, 90)),
                ),
            ]))
        }).collect();
        let mut mstate = ListState::default();
        if ob.provider_focus == ProviderFocus::ModelList {
            mstate.select(Some(ob.model_cursor));
        }
        f.render_stateful_widget(
            List::new(items)
                .highlight_style(Style::default().bg(Color::Rgb(20, 28, 48))),
            model_inner,
            &mut mstate,
        );
    }

    let key_border = if ob.provider_focus == ProviderFocus::ApiKeyInput {
        ACCENT
    } else {
        DIM
    };
    let key_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(key_border))
        .title(Span::styled(
            " API Key ",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));
    let key_inner = key_block.inner(cols[2]);
    f.render_widget(key_block, cols[2]);

    if is_ollama {
        f.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No API key needed.",
                    Style::default().fg(Color::Rgb(80, 200, 120)),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Ollama runs locally",
                    Style::default().fg(DIM),
                )),
                Line::from(Span::styled(
                    "  on this device, free.",
                    Style::default().fg(DIM),
                )),
            ]),
            key_inner,
        );
    } else {
        let key_rows = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(key_inner);

        let p_color = ob.current_provider()
            .map(|p| p.color)
            .unwrap_or(Color::White);
        f.render_widget(
            Paragraph::new(Span::styled(
                ob.current_provider().map(|p| p.label).unwrap_or(""),
                Style::default().fg(p_color).add_modifier(Modifier::BOLD),
            )),
            key_rows[0],
        );

        let url = ob.key_hint_url();
        if !url.is_empty() {
            f.render_widget(
                Paragraph::new(Span::styled(
                    url,
                    Style::default()
                        .fg(Color::Rgb(80, 140, 255))
                        .add_modifier(Modifier::UNDERLINED),
                )),
                key_rows[1],
            );
        }

        let raw_len = ob.api_key_input.len();
        let display = if raw_len == 0 {
            "paste key here...".to_string()
        } else {
            format!(
                "{}{}",
                "•".repeat(raw_len.saturating_sub(4)),
                &ob.api_key_input[raw_len.saturating_sub(4)..]
            )
        };
        let input_color = match &ob.key_status {
            KeyStatus::Valid => Color::Rgb(80, 200, 130),
            KeyStatus::Invalid(_) => Color::Rgb(220, 80, 80),
            _ => if raw_len > 0 { Color::Rgb(120, 200, 140) } else { DIM },
        };
        f.render_widget(
            Paragraph::new(Span::styled(
                if raw_len == 0 { "paste key here...".to_string() } else { display },
                Style::default().fg(input_color),
            ))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(
                        if ob.provider_focus == ProviderFocus::ApiKeyInput {
                            input_color
                        } else {
                            DIM
                        },
                    ))
                    .style(Style::default().bg(Color::Rgb(10, 14, 28))),
            ),
            key_rows[3],
        );

        let (status_text, status_color) = match &ob.key_status {
            KeyStatus::Empty   => ("".to_string(), DIM),
            KeyStatus::Typing  => (format!("{} chars", raw_len), Color::Rgb(140, 150, 180)),
            KeyStatus::Valid   => ("✓  Key verified".to_string(), Color::Rgb(80, 200, 130)),
            KeyStatus::Invalid(msg) => (format!("✗  {}", msg), Color::Rgb(220, 80, 80)),
        };
        f.render_widget(
            Paragraph::new(Span::styled(status_text, Style::default().fg(status_color))),
            key_rows[4],
        );

        if ob.provider_focus == ProviderFocus::ApiKeyInput {
            let cx = key_rows[3].x + 1 + (raw_len as u16).min(key_rows[3].width.saturating_sub(3));
            f.set_cursor(cx, key_rows[3].y + 1);
        }
    }
}

fn draw_ob_memory(
    f: &mut Frame,
    ob: &crate::tui::onboarding::OnboardingState,
    area: Rect,
) {
    let popup = centered_rect(55, 60, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(60, 100, 160)))
        .title(Span::styled(
            " Step 3 — Session memory ",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let icon = if ob.remember_sessions { "●" } else { "○" };
    let color = if ob.remember_sessions {
        Color::Rgb(80, 200, 120)
    } else {
        Color::Rgb(150, 150, 170)
    };
    let label = if ob.remember_sessions {
        "Remember conversations  (on)"
    } else {
        "Remember conversations  (off)"
    };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Should Antflow remember your conversation history",
            Style::default().fg(Color::Rgb(180, 190, 210)),
        )),
        Line::from(Span::styled(
            "between sessions?",
            Style::default().fg(Color::Rgb(180, 190, 210)),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("  {}  ", icon),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  Press Space to toggle, Enter to continue.",
            Style::default().fg(DIM),
        )),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_ob_done(
    f: &mut Frame,
    ob: &crate::tui::onboarding::OnboardingState,
    area: Rect,
) {
    let popup = centered_rect(60, 75, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(80, 200, 120)))
        .title(Span::styled(
            " ✓ Setup complete ",
            Style::default()
                .fg(Color::Rgb(80, 200, 120))
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let storage_label = match ob.chosen_storage {
        crate::config::StorageLocation::Portable => "Portable (next to binary)",
        crate::config::StorageLocation::Global   => "Global (~/.config/antflow)",
    };
    let memory_label = if ob.remember_sessions { "on" } else { "off" };

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Your choices",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Storage   ", Style::default().fg(DIM)),
            Span::styled(storage_label, Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Provider  ", Style::default().fg(DIM)),
            Span::styled(ob.chosen_provider.clone(), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Model     ", Style::default().fg(DIM)),
            Span::styled(ob.chosen_model.clone(), Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("  Memory    ", Style::default().fg(DIM)),
            Span::styled(memory_label, Style::default().fg(Color::White)),
        ]),
        Line::from(""),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  Press Enter to launch Antflow  ",
                Style::default()
                    .fg(Color::Rgb(10, 14, 28))
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}