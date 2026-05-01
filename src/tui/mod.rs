use crate::daemon::config::Config;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap},
};
use reqwest::Client;
use serde::Deserialize;
use std::{
    io,
    time::{Duration, Instant},
};

const POLL_INTERVAL_MS: u64 = 2000;
const CHAT_POLL_INTERVAL_MS: u64 = 500;

// ── API response types ─────────────────────────

#[derive(Deserialize, Debug, Clone)]
struct StatusResponse {
    node_id: Option<String>,
    mode: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct Peer {
    node_id: String,
}

#[derive(Deserialize, Debug, Clone)]
struct ChatMessage {
    from: String,
    text: String,
    timestamp: u64,
}

// ── UI state ──────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum InputMode {
    Normal,
    Connecting,
    ChatBrowse,                    // navigating peer list to pick a chat target
    Chatting { peer_id: String },  // active chat screen
    ChatInput { peer_id: String }, // typing a message
}

#[derive(Debug, Clone)]
struct ChatEntry {
    from_self: bool,
    text: String,
}

#[derive(Debug, Clone)]
struct App {
    node_id: String,
    mode_label: String,
    peers: Vec<String>,
    peer_cursor: usize, // selected index in peers list
    input_mode: InputMode,
    input_buf: String,
    status_msg: Option<String>,
    last_poll: Instant,
    last_chat_poll: Instant,
    chat_history: Vec<ChatEntry>, // messages for the active chat session
    client: Client,
    api_base: String,
    api_key: String,
}

impl App {
    fn new(api_base: String, api_key: String) -> Self {
        Self {
            node_id: "connecting...".to_string(),
            mode_label: String::new(),
            peers: Vec::new(),
            peer_cursor: 0,
            input_mode: InputMode::Normal,
            input_buf: String::new(),
            status_msg: None,
            last_poll: Instant::now() - Duration::from_secs(10),
            last_chat_poll: Instant::now(),
            chat_history: Vec::new(),
            client: Client::new(),
            api_base,
            api_key,
        }
    }

    // ── API calls ────────────────────────────────────────────────────────────

    async fn fetch_status(&mut self) {
        let url = format!("{}/status", self.api_base);
        match self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
        {
            Ok(resp) => {
                let body = resp.text().await.unwrap_or_default();
                if body.starts_with("NodeID:") {
                    self.node_id = body.trim_start_matches("NodeID:").trim().to_string();
                    self.mode_label = "iroh active".to_string();
                } else {
                    self.node_id = "—".to_string();
                    self.mode_label = "reticulum-only".to_string();
                }
            }
            Err(_) => {
                self.node_id = "daemon unreachable".to_string();
                self.mode_label = String::new();
            }
        }
    }

    async fn fetch_peers(&mut self) {
        let url = format!("{}/peers", self.api_base);
        match self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
        {
            Ok(resp) => {
                let body = resp.text().await.unwrap_or_default();
                if let Ok(ids) = serde_json::from_str::<Vec<String>>(&body) {
                    // clamp cursor if peer list shrank
                    if !ids.is_empty() {
                        self.peer_cursor = self.peer_cursor.min(ids.len() - 1);
                    }
                    self.peers = ids;
                } else {
                    self.peers = Vec::new();
                }
            }
            Err(_) => {}
        }
    }

    async fn connect_to(&mut self, node_id: &str) {
        let url = format!("{}/connect", self.api_base);
        let body = serde_json::json!({ "node_id": node_id });
        match self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                if status.is_success() {
                    self.status_msg = Some(format!("✓ {}", text));
                    self.fetch_peers().await;
                } else {
                    self.status_msg = Some(format!("✗ {}", text));
                }
            }
            Err(e) => {
                self.status_msg = Some(format!("✗ request failed: {}", e));
            }
        }
    }

    async fn send_chat(&mut self, peer_id: &str, message: &str) {
        let url = format!("{}/chat/send", self.api_base);
        let body = serde_json::json!({ "node_id": peer_id, "message": message });
        match self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    self.chat_history.push(ChatEntry {
                        from_self: true,
                        text: message.to_string(),
                    });
                } else {
                    let text = resp.text().await.unwrap_or_default();
                    self.status_msg = Some(format!("✗ {}", text));
                }
            }
            Err(e) => {
                self.status_msg = Some(format!("✗ send failed: {}", e));
            }
        }
    }

    async fn poll_chat_inbox(&mut self) {
        let url = format!("{}/chat/messages", self.api_base);
        match self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
        {
            Ok(resp) => {
                let body = resp.text().await.unwrap_or_default();
                if let Ok(messages) = serde_json::from_str::<Vec<ChatMessage>>(&body) {
                    for msg in messages {
                        self.chat_history.push(ChatEntry {
                            from_self: false,
                            text: format!("{}: {}", &msg.from[..8.min(msg.from.len())], msg.text),
                        });
                    }
                }
            }
            Err(_) => {}
        }
        self.last_chat_poll = Instant::now();
    }
}

// ── Rendering ─────────────────────────

fn ui(f: &mut Frame, app: &App) {
    match &app.input_mode {
        InputMode::Chatting { peer_id } | InputMode::ChatInput { peer_id } => {
            render_chat_screen(f, app, peer_id.clone());
        }
        _ => {
            render_main_screen(f, app);
        }
    }
}

fn inner_rect(r: Rect) -> Rect {
    Rect {
        x: r.x + 1,
        y: r.y + 1,
        width: r.width.saturating_sub(2),
        height: r.height.saturating_sub(2),
    }
}

// ── Main screen ───────────────────────

fn render_main_screen(f: &mut Frame, app: &App) {
    let area = f.area();

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Red))
        .title(Span::styled(
            " aerowan ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center);
    f.render_widget(outer, area);

    let inner = inner_rect(area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(inner);

    render_header(f, app, chunks[0]);
    render_peers(f, app, chunks[1]);
    render_input(f, app, chunks[2]);
    render_hints(f, app, chunks[3]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let node_id_display = if app.node_id.len() > 24 {
        format!("{}…", &app.node_id[..24])
    } else {
        app.node_id.clone()
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("node  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &node_id_display,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("mode  ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.mode_label, Style::default().fg(Color::Green)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}

fn render_peers(f: &mut Frame, app: &App, area: Rect) {
    let in_browse = app.input_mode == InputMode::ChatBrowse;

    let block = Block::default()
        .title(Span::styled(
            format!(" peers ({}) ", app.peers.len()),
            Style::default().fg(Color::Cyan),
        ))
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(if in_browse {
            Color::Yellow
        } else {
            Color::DarkGray
        }));

    if app.peers.is_empty() {
        let para = Paragraph::new(Span::styled(
            "no connections",
            Style::default().fg(Color::DarkGray),
        ))
        .block(block);
        f.render_widget(para, area);
    } else {
        let items: Vec<ListItem> = app
            .peers
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let short = if id.len() > 32 {
                    format!("{}…", &id[..32])
                } else {
                    id.clone()
                };
                let selected = in_browse && i == app.peer_cursor;
                let style = if selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        "● ",
                        Style::default().fg(if selected { Color::Black } else { Color::Green }),
                    ),
                    Span::styled(short, style),
                ]))
            })
            .collect();

        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }
}

fn render_input(f: &mut Frame, app: &App, area: Rect) {
    match &app.input_mode {
        InputMode::Connecting => {
            let block = Block::default()
                .title(Span::styled(
                    " connect — paste NodeID, enter to dial, esc to cancel ",
                    Style::default().fg(Color::Yellow),
                ))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Yellow));

            let para = Paragraph::new(app.input_buf.as_str())
                .style(Style::default().fg(Color::White))
                .block(block);
            f.render_widget(para, area);
            f.set_cursor_position((area.x + 1 + app.input_buf.len() as u16, area.y + 1));
        }
        InputMode::Normal | InputMode::ChatBrowse => {
            if let Some(msg) = &app.status_msg {
                let colour = if msg.starts_with('✓') {
                    Color::Green
                } else {
                    Color::Red
                };
                let para = Paragraph::new(Span::styled(msg, Style::default().fg(colour)))
                    .wrap(Wrap { trim: true });
                f.render_widget(para, area);
            }
        }
        _ => {}
    }
}

fn render_hints(f: &mut Frame, app: &App, area: Rect) {
    let hints = match &app.input_mode {
        InputMode::Normal => vec![
            Span::styled(
                "[c]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" connect  "),
            Span::styled(
                "[t]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" chat  "),
            Span::styled(
                "[q]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" quit"),
        ],
        InputMode::Connecting => vec![
            Span::styled(
                "[enter]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" dial  "),
            Span::styled(
                "[esc]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" cancel"),
        ],
        InputMode::ChatBrowse => vec![
            Span::styled(
                "[↑↓]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" select  "),
            Span::styled(
                "[enter]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" open chat  "),
            Span::styled(
                "[esc]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" cancel"),
        ],
        _ => vec![],
    };

    let para = Paragraph::new(Line::from(hints)).alignment(Alignment::Center);
    f.render_widget(para, area);
}

// ── Chat screen ───────────────────────

fn render_chat_screen(f: &mut Frame, app: &App, peer_id: String) {
    let area = f.area();

    let short_peer = if peer_id.len() > 16 {
        format!("{}…", &peer_id[..16])
    } else {
        peer_id.clone()
    };

    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Red))
        .title(Span::styled(
            format!(" chat ↔ {} ", short_peer),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center);
    f.render_widget(outer, area);

    let inner = inner_rect(area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // message history
            Constraint::Length(3), // input bar
            Constraint::Length(1), // hints
        ])
        .split(inner);

    render_chat_history(f, app, chunks[0]);
    render_chat_input(f, app, chunks[1]);
    render_chat_hints(f, app, chunks[2]);
}

fn render_chat_history(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

    let items: Vec<ListItem> = app
        .chat_history
        .iter()
        .map(|entry| {
            if entry.from_self {
                ListItem::new(Line::from(vec![
                    Span::styled("  you  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&entry.text, Style::default().fg(Color::Cyan)),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::styled(" them  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(&entry.text, Style::default().fg(Color::White)),
                ]))
            }
        })
        .collect();

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn render_chat_input(f: &mut Frame, app: &App, area: Rect) {
    let is_typing = matches!(app.input_mode, InputMode::ChatInput { .. });

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if is_typing {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
        .title(Span::styled(
            if is_typing {
                " message "
            } else {
                " press [i] to type "
            },
            Style::default().fg(Color::DarkGray),
        ));

    let para = Paragraph::new(app.input_buf.as_str())
        .style(Style::default().fg(Color::White))
        .block(block);
    f.render_widget(para, area);

    if is_typing {
        f.set_cursor_position((area.x + 1 + app.input_buf.len() as u16, area.y + 1));
    }
}

fn render_chat_hints(f: &mut Frame, app: &App, area: Rect) {
    let hints = match &app.input_mode {
        InputMode::Chatting { .. } => vec![
            Span::styled(
                "[i]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" type  "),
            Span::styled(
                "[esc]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" back"),
        ],
        InputMode::ChatInput { .. } => vec![
            Span::styled(
                "[enter]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" send  "),
            Span::styled(
                "[esc]",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" cancel"),
        ],
        _ => vec![],
    };

    let para = Paragraph::new(Line::from(hints)).alignment(Alignment::Center);
    f.render_widget(para, area);
}

// ── Main event loop ───────────────────────────

pub async fn run() -> anyhow::Result<()> {
    let (config, config_dir) =
        Config::load().map_err(|e| anyhow::anyhow!("failed to load config: {}", e))?;

    let api_key = crate::utils::identity::load_api_key(&config_dir)
        .map_err(|e| anyhow::anyhow!("failed to load API key: {}", e))?;

    let api_base = format!("http://127.0.0.1:{}", config.api.port);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(api_base, api_key);
    app.fetch_status().await;
    app.fetch_peers().await;

    loop {
        terminal.draw(|f| ui(f, &app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app.input_mode.clone() {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c') => {
                            app.input_mode = InputMode::Connecting;
                            app.input_buf.clear();
                            app.status_msg = None;
                        }
                        KeyCode::Char('t') => {
                            if !app.peers.is_empty() {
                                app.input_mode = InputMode::ChatBrowse;
                            } else {
                                app.status_msg = Some("✗ no peers connected".to_string());
                            }
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break;
                        }
                        _ => {}
                    },
                    InputMode::Connecting => match key.code {
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.input_buf.clear();
                        }
                        KeyCode::Enter => {
                            let node_id = app.input_buf.trim().to_string();
                            if !node_id.is_empty() {
                                app.connect_to(&node_id).await;
                            }
                            app.input_mode = InputMode::Normal;
                            app.input_buf.clear();
                        }
                        KeyCode::Backspace => {
                            app.input_buf.pop();
                        }
                        KeyCode::Char(c) => {
                            app.input_buf.push(c);
                        }
                        _ => {}
                    },
                    InputMode::ChatBrowse => match key.code {
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                        }
                        KeyCode::Up => {
                            if app.peer_cursor > 0 {
                                app.peer_cursor -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if app.peer_cursor + 1 < app.peers.len() {
                                app.peer_cursor += 1;
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(peer_id) = app.peers.get(app.peer_cursor).cloned() {
                                app.chat_history.clear();
                                app.input_mode = InputMode::Chatting { peer_id };
                            }
                        }
                        _ => {}
                    },
                    InputMode::Chatting { peer_id } => match key.code {
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.chat_history.clear();
                        }
                        KeyCode::Char('i') => {
                            app.input_mode = InputMode::ChatInput {
                                peer_id: peer_id.clone(),
                            };
                            app.input_buf.clear();
                        }
                        _ => {}
                    },
                    InputMode::ChatInput { peer_id } => match key.code {
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Chatting {
                                peer_id: peer_id.clone(),
                            };
                            app.input_buf.clear();
                        }
                        KeyCode::Enter => {
                            let message = app.input_buf.trim().to_string();
                            if !message.is_empty() {
                                app.send_chat(&peer_id, &message).await;
                            }
                            app.input_mode = InputMode::Chatting {
                                peer_id: peer_id.clone(),
                            };
                            app.input_buf.clear();
                        }
                        KeyCode::Backspace => {
                            app.input_buf.pop();
                        }
                        KeyCode::Char(c) => {
                            app.input_buf.push(c);
                        }
                        _ => {}
                    },
                }
            }
        }

        // Periodic background refresh
        if app.last_poll.elapsed() >= Duration::from_millis(POLL_INTERVAL_MS) {
            app.fetch_status().await;
            app.fetch_peers().await;
            app.last_poll = Instant::now();
        }

        // Chat inbox polling — only when chat screen is active
        let in_chat = matches!(
            app.input_mode,
            InputMode::Chatting { .. } | InputMode::ChatInput { .. }
        );
        if in_chat && app.last_chat_poll.elapsed() >= Duration::from_millis(CHAT_POLL_INTERVAL_MS) {
            app.poll_chat_inbox().await;
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
