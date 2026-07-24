//! Ratatui terminal UI to scaffold/edit the YAML config and detect the path to
//! the credential manager (`gh`).

use crate::config::{self, Config, Mapping};
use crate::gh;
use anyhow::Result;
use std::path::PathBuf;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

#[derive(PartialEq)]
enum Mode {
    List,
    Edit,
    ConfirmQuit,
}

struct Form {
    editing: Option<usize>, // None = adding a new mapping
    fields: [String; 3],    // host, owner, account
    field: usize,
}

impl Form {
    fn new(editing: Option<usize>, host: &str, owner: &str, account: &str) -> Self {
        Form {
            editing,
            fields: [host.to_string(), owner.to_string(), account.to_string()],
            field: 0,
        }
    }
}

struct App {
    cfg: Config,
    path: PathBuf,
    selected: usize,
    list_state: ListState,
    mode: Mode,
    form: Form,
    status: String,
    dirty: bool,
    quit: bool,
}

impl App {
    fn new(cfg: Config, path: PathBuf) -> Self {
        App {
            cfg,
            path,
            selected: 0,
            list_state: ListState::default(),
            mode: Mode::List,
            form: Form::new(None, "", "", ""),
            status: "Ready.".to_string(),
            dirty: false,
            quit: false,
        }
    }

    fn clamp_selection(&mut self) {
        let len = self.cfg.mappings.len();
        if len == 0 {
            self.selected = 0;
            self.list_state.select(None);
        } else {
            if self.selected >= len {
                self.selected = len - 1;
            }
            self.list_state.select(Some(self.selected));
        }
    }

    fn save(&mut self) {
        match self.cfg.save_to(&self.path) {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("Saved to {}", self.path.display());
            }
            Err(e) => self.status = format!("Save failed: {e:#}"),
        }
    }

    fn detect_gh(&mut self) {
        match gh::locate_gh(None) {
            Some(p) => {
                let s = p.to_string_lossy().into_owned();
                self.cfg.gh_path = Some(s.clone());
                self.dirty = true;
                self.status = format!("Detected gh: {s}");
            }
            None => self.status = "gh not found on PATH or well-known locations.".to_string(),
        }
    }

    // ---- key handling -----------------------------------------------------

    fn on_key(&mut self, code: KeyCode) {
        match self.mode {
            Mode::List => self.on_key_list(code),
            Mode::Edit => self.on_key_edit(code),
            Mode::ConfirmQuit => self.on_key_confirm(code),
        }
    }

    fn on_key_list(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') => {
                if self.dirty {
                    self.mode = Mode::ConfirmQuit;
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') if !self.cfg.mappings.is_empty() => {
                self.selected = (self.selected + 1).min(self.cfg.mappings.len() - 1);
            }
            KeyCode::Char('a') => {
                self.form = Form::new(None, "github.com", "", "");
                self.mode = Mode::Edit;
                self.status = "Adding mapping — Tab to move, Enter to save, Esc to cancel.".into();
            }
            KeyCode::Char('e') => {
                if let Some(m) = self.cfg.mappings.get(self.selected) {
                    self.form = Form::new(Some(self.selected), &m.host, &m.owner, &m.account);
                    self.mode = Mode::Edit;
                    self.status = "Editing mapping — Tab to move, Enter to save, Esc to cancel.".into();
                } else {
                    self.status = "Nothing to edit.".into();
                }
            }
            KeyCode::Char('d') if self.selected < self.cfg.mappings.len() => {
                let m = self.cfg.mappings.remove(self.selected);
                self.dirty = true;
                self.selected = self.selected.min(self.cfg.mappings.len().saturating_sub(1));
                self.status = format!("Deleted {}/{}.", m.host, m.owner);
            }
            KeyCode::Char('g') => self.detect_gh(),
            KeyCode::Char('s') => self.save(),
            _ => {}
        }
    }

    fn on_key_edit(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => {
                self.mode = Mode::List;
                self.status = "Cancelled.".into();
            }
            KeyCode::Tab | KeyCode::Down => {
                self.form.field = (self.form.field + 1) % 3;
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.form.field = (self.form.field + 2) % 3;
            }
            KeyCode::Backspace => {
                self.form.fields[self.form.field].pop();
            }
            KeyCode::Enter => self.commit_form(),
            KeyCode::Char(c) => {
                self.form.fields[self.form.field].push(c);
            }
            _ => {}
        }
    }

    fn commit_form(&mut self) {
        let host = self.form.fields[0].trim().to_string();
        let owner = self.form.fields[1].trim().to_string();
        let account = self.form.fields[2].trim().to_string();
        if host.is_empty() || owner.is_empty() || account.is_empty() {
            self.status = "All fields are required (owner may be '*').".into();
            return;
        }
        let mapping = Mapping {
            host,
            owner,
            account,
        };
        match self.form.editing {
            Some(i) if i < self.cfg.mappings.len() => {
                self.cfg.mappings[i] = mapping;
                self.selected = i;
            }
            _ => {
                self.cfg.mappings.push(mapping);
                self.selected = self.cfg.mappings.len() - 1;
            }
        }
        self.dirty = true;
        self.mode = Mode::List;
        self.status = "Mapping saved (press 's' to write to disk).".into();
    }

    fn on_key_confirm(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.save();
                self.quit = true;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => self.quit = true,
            KeyCode::Char('c') | KeyCode::Esc => {
                self.mode = Mode::List;
                self.status = "Quit cancelled.".into();
            }
            _ => {}
        }
    }
}

/// Launch the TUI.
pub fn run() -> Result<()> {
    let path = config::config_path()?;
    let cfg = Config::load_from(&path).unwrap_or_default();
    let mut app = App::new(cfg, path);

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        app.clamp_selection();
        terminal.draw(|f| ui(f, app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                app.on_key(key.code);
            }
        }
        if app.quit {
            break;
        }
    }
    Ok(())
}

// ---- rendering ------------------------------------------------------------

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(3),
            Constraint::Length(4),
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);
    render_list(f, app, chunks[1]);
    render_footer(f, app, chunks[2]);

    if app.mode == Mode::Edit {
        render_edit(f, app);
    } else if app.mode == Mode::ConfirmQuit {
        render_confirm(f);
    }
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let gh = app
        .cfg
        .gh_path
        .clone()
        .unwrap_or_else(|| "(not set — press 'g' to detect)".to_string());
    let dirty = if app.dirty { "  [modified]" } else { "" };
    let lines = vec![
        Line::from(Span::styled(
            format!("gh-autoswitch — config editor{dirty}"),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!("gh path : {gh}")),
        Line::from(format!("config  : {}", app.path.display())),
    ];
    let p = Paragraph::new(lines).block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}

fn render_list(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = if app.cfg.mappings.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "(no mappings — press 'a' to add one)",
            Style::default().fg(Color::DarkGray),
        )))]
    } else {
        app.cfg
            .mappings
            .iter()
            .map(|m| {
                ListItem::new(Line::from(format!(
                    "{} / {}  →  {}",
                    m.host, m.owner, m.account
                )))
            })
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Mappings (host / owner → account) "),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    if app.cfg.mappings.is_empty() {
        f.render_widget(list, area);
    } else {
        f.render_stateful_widget(list, area, &mut app.list_state);
    }
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let keys = "[a]dd  [e]dit  [d]elete  [g] detect gh  [s]ave  [q]uit";
    let lines = vec![
        Line::from(Span::styled(keys, Style::default().fg(Color::Yellow))),
        Line::from(Span::styled(
            format!("status: {}", app.status),
            Style::default().fg(Color::Green),
        )),
    ];
    let p = Paragraph::new(lines).block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}

fn render_edit(f: &mut Frame, app: &App) {
    let area = centered_rect(64, 40, f.area());
    f.render_widget(Clear, area);

    let labels = ["Host  ", "Owner ", "Account"];
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    for (i, label) in labels.iter().enumerate() {
        let active = i == app.form.field;
        let value = &app.form.fields[i];
        let shown = if active {
            format!("{value}_")
        } else {
            value.clone()
        };
        let style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::raw(format!("  {label} : ")),
            Span::styled(format!(" {shown} "), style),
        ]));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "  Tab/↑↓ move   Enter save   Esc cancel",
        Style::default().fg(Color::DarkGray),
    )));

    let title = if app.form.editing.is_some() {
        " Edit mapping "
    } else {
        " Add mapping "
    };
    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    f.render_widget(p, area);
}

fn render_confirm(f: &mut Frame) {
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);
    let p = Paragraph::new(vec![
        Line::from(""),
        Line::from("  Unsaved changes. Save before quitting?"),
        Line::from(""),
        Line::from(Span::styled(
            "  [y] save & quit   [n] discard & quit   [c] cancel",
            Style::default().fg(Color::Yellow),
        )),
    ])
    .alignment(Alignment::Left)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Quit ")
            .border_style(Style::default().fg(Color::Red)),
    );
    f.render_widget(p, area);
}

/// A centered rectangle occupying `pct_x`% × `pct_y`% of `area`.
fn centered_rect(pct_x: u16, pct_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vertical[1])[1]
}
