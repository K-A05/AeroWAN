use aerowan::daemon::config::Config;
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

// ── API response types ────────────────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone)]
struct StatusResponse {
    node_id: Option<String>,
    mode: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct Peer {
    node_id: String,
}

// ── UI state ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum InputMode {
    Normal,
    Connecting, // user is typing a NodeID
}

#[derive(Debug, Clone)]
struct App {
    node_id: String,
    mode_label: String,
    peers: Vec<String>,
    input_mode: InputMode,
    input_buf: String,
    status_msg: Option<String>,
    last_poll: Instant,
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
            input_mode: InputMode::Normal,
            input_buf: String::new(),
            status_msg: None,
            last_poll: Instant::now() - Duration::from_secs(10),
            client: Client::new(),
            api_base,
            api_key,
        }
    }

    // ── API calls (blocking via tokio runtime) ────────────────────────────────

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
                // /status returns plain text: "NodeID: <id>" or the iroh-disabled message
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
                // currently returns "[]" or a JSON array of node IDs
                let body = resp.text().await.unwrap_or_default();
                if let Ok(ids) = serde_json::from_str::<Vec<String>>(&body) {
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
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn ui(f: &mut Frame, app: &App) {
    let area = f.area();

    // Outer border
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " aerowan ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center);
    f.render_widget(outer, area);

    // Inner layout: header | peers | input/status | footer
    let inner = inner_rect(area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header (NodeID + mode)
            Constraint::Min(5),    // peers list
            Constraint::Length(3), // input / status bar
            Constraint::Length(1), // key hints
        ])
        .split(inner);

    render_header(f, app, chunks[0]);
    render_peers(f, app, chunks[1]);
    render_input(f, app, chunks[2]);
    render_hints(f, app, chunks[3]);
}

fn inner_rect(r: Rect) -> Rect {
    Rect {
        x: r.x + 1,
        y: r.y + 1,
        width: r.width.saturating_sub(2),
        height: r.height.saturating_sub(2),
    }
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
    let block = Block::default()
        .title(Span::styled(
            format!(" peers ({}) ", app.peers.len()),
            Style::default().fg(Color::Cyan),
        ))
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::DarkGray));

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
            .map(|id| {
                let short = if id.len() > 32 {
                    format!("{}…", &id[..32])
                } else {
                    id.clone()
                };
                ListItem::new(Line::from(vec![
                    Span::styled("● ", Style::default().fg(Color::Green)),
                    Span::styled(short, Style::default().fg(Color::White)),
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

            // show cursor
            f.set_cursor_position((area.x + 1 + app.input_buf.len() as u16, area.y + 1));
        }
        InputMode::Normal => {
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
    }
}

fn render_hints(f: &mut Frame, app: &App, area: Rect) {
    let hints = match app.input_mode {
        InputMode::Normal => vec![
            Span::styled(
                "[c]",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" connect  "),
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
    };

    let para = Paragraph::new(Line::from(hints)).alignment(Alignment::Center);
    f.render_widget(para, area);
}

// ── Main event loop ───────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load config the same way the daemon does
    let (config, config_dir) =
        Config::load().map_err(|e| anyhow::anyhow!("failed to load config: {}", e))?;

    let api_key = aerowan::utils::identity::load_api_key(&config_dir)
        .map_err(|e| anyhow::anyhow!("failed to load API key: {}", e))?;

    let api_base = format!("http://127.0.0.1:{}", config.api.port);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(api_base, api_key);

    // initial data fetch
    app.fetch_status().await;
    app.fetch_peers().await;

    loop {
        terminal.draw(|f| ui(f, &app))?;

        // poll for input with a short timeout so we can also tick the refresh
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c') => {
                            app.input_mode = InputMode::Connecting;
                            app.input_buf.clear();
                            app.status_msg = None;
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
                }
            }
        }

        // periodic background refresh
        if app.last_poll.elapsed() >= Duration::from_millis(POLL_INTERVAL_MS) {
            app.fetch_status().await;
            app.fetch_peers().await;
            app.last_poll = Instant::now();
        }
    }

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
