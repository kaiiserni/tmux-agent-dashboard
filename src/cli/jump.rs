//! `jump`: a small `display-popup` picker over the pending agents.
//!
//! `prefix + n` opens it. It lists the same pending panes as the bottom
//! status bar, numbered, with a countdown: leave it alone and it jumps to
//! the most urgent one (the old `prefix + n` behaviour); type a digit, or
//! navigate with j/k and hit Enter, to pick a different one. Any
//! navigation key cancels the countdown so you can take your time.

use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph},
};

use crate::navigate;
use crate::pending::{self, PendingEntry, Priority};
use crate::tmux;
use crate::ui::colors::ColorTheme;

const JUMP_TIMEOUT_MS: &str = "@dashboard_jump_timeout_ms";
const DEFAULT_TIMEOUT_MS: u64 = 500;

/// Restores the terminal on drop, mirroring `main.rs`'s `TuiSession` but
/// without mouse capture (the picker is keyboard-only).
struct JumpTui;

impl JumpTui {
    fn enter(stdout: &mut io::Stdout) -> io::Result<Self> {
        enable_raw_mode()?;
        if let Err(err) = execute!(stdout, EnterAlternateScreen, Hide) {
            let _ = disable_raw_mode();
            return Err(err);
        }
        Ok(Self)
    }
}

impl Drop for JumpTui {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, Show, LeaveAlternateScreen);
    }
}

enum Outcome {
    Jump(String),
    Cancel,
}

pub fn cmd_jump() -> io::Result<()> {
    let entries = pending::collect_pending();
    if entries.is_empty() {
        let _ = tmux::run_tmux(&["display-message", "No pending agents"]);
        return Ok(());
    }
    // One candidate: skip the popup, just go there.
    if entries.len() == 1 {
        navigate::jump_to(&entries[0].pane_id);
        return Ok(());
    }

    let timeout_ms = tmux::get_option(JUMP_TIMEOUT_MS)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_TIMEOUT_MS);

    let theme = ColorTheme::from_tmux();

    let mut stdout = io::stdout();
    let _guard = JumpTui::enter(&mut stdout)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let total = entries.len();
    let mut selected = 0usize;
    let mut countdown = if timeout_ms > 0 {
        Some((Instant::now(), Duration::from_millis(timeout_ms)))
    } else {
        None
    };

    let outcome = loop {
        // Countdown expiry → jump to the highlighted entry (starts on the
        // most urgent, so an untouched popup == the old `prefix + n`).
        if let Some((start, dur)) = countdown
            && start.elapsed() >= dur
        {
            break Outcome::Jump(entries[selected].pane_id.clone());
        }

        let remaining = countdown.map(|(start, dur)| dur.saturating_sub(start.elapsed()));
        terminal.draw(|f| {
            render(f, &entries, selected, remaining, timeout_ms, &theme);
        })?;

        let poll = if countdown.is_some() {
            Duration::from_millis(80)
        } else {
            Duration::from_millis(250)
        };
        if !event::poll(poll)? {
            continue;
        }
        let Event::Key(k) = event::read()? else {
            continue;
        };
        if k.kind == KeyEventKind::Release {
            continue;
        }
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => break Outcome::Cancel,
            KeyCode::Enter => break Outcome::Jump(entries[selected].pane_id.clone()),
            KeyCode::Char('n') => break Outcome::Jump(next_target(&entries).to_string()),
            KeyCode::Char('j') | KeyCode::Down => {
                if selected + 1 < total {
                    selected += 1;
                }
                countdown = None;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                selected = selected.saturating_sub(1);
                countdown = None;
            }
            KeyCode::Char('g') | KeyCode::Home => {
                selected = 0;
                countdown = None;
            }
            KeyCode::Char('G') | KeyCode::End => {
                selected = total - 1;
                countdown = None;
            }
            KeyCode::Char(c @ '1'..='9') => {
                let idx = (c as u8 - b'1') as usize;
                if idx < total {
                    break Outcome::Jump(entries[idx].pane_id.clone());
                }
                countdown = None;
            }
            _ => {}
        }
    };

    if let Outcome::Jump(pane) = outcome {
        navigate::jump_to(&pane);
    }
    Ok(())
}

/// `n` / timeout default: the most urgent entry that isn't a user-parked
/// Marked-Unread one, matching the `next` subcommand. Falls back to the
/// first entry when everything is marked.
fn next_target(entries: &[PendingEntry]) -> &str {
    entries
        .iter()
        .find(|e| e.priority != Priority::MarkedUnread)
        .map(|e| e.pane_id.as_str())
        .unwrap_or(&entries[0].pane_id)
}

fn priority_glyph(p: Priority) -> (&'static str, u8) {
    match p {
        Priority::Attention => ("▲", 167),
        Priority::Error => ("✕", 167),
        Priority::Waiting => ("◐", 221),
        Priority::Responded => ("↩", 221),
        Priority::MarkedUnread => ("📌", 117),
    }
}

fn render(
    f: &mut ratatui::Frame,
    entries: &[PendingEntry],
    selected: usize,
    remaining: Option<Duration>,
    timeout_ms: u64,
    theme: &ColorTheme,
) {
    use ratatui::style::Color;

    let area = f.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .padding(Padding::horizontal(1))
        .title(Span::styled(
            " Jump to agent ",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(inner);

    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (icon, icon_color) = priority_glyph(e.priority);
            let num = if i < 9 {
                format!("{} ", i + 1)
            } else {
                "· ".to_string()
            };
            let mut spans = vec![
                Span::styled(num, Style::default().fg(theme.text_inactive)),
                Span::styled(icon, Style::default().fg(Color::Indexed(icon_color))),
                Span::raw(" "),
                Span::styled(
                    e.agent.glyph(),
                    Style::default().fg(theme.agent_color(&e.agent)),
                ),
                Span::raw(" "),
                Span::styled(e.repo.clone(), Style::default().fg(theme.text_active)),
                Span::styled(format!("/{}", e.label), Style::default().fg(theme.branch)),
            ];
            if !e.wait_reason.is_empty() {
                spans.push(Span::styled(
                    format!("  ({})", e.wait_reason),
                    Style::default().fg(theme.wait_reason),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .highlight_symbol("▌ ")
        .highlight_style(
            Style::default()
                .bg(theme.selection_bg)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default();
    state.select(Some(selected));
    f.render_stateful_widget(list, rows[0], &mut state);

    f.render_widget(footer(remaining, timeout_ms, rows[1].width as usize, theme), rows[1]);
}

fn footer(
    remaining: Option<Duration>,
    timeout_ms: u64,
    width: usize,
    theme: &ColorTheme,
) -> Paragraph<'static> {
    let bar = match remaining {
        Some(rem) if timeout_ms > 0 => {
            let bar_w = width.max(8);
            let frac = rem.as_millis() as f64 / timeout_ms as f64;
            let filled = (frac * bar_w as f64).round() as usize;
            let filled = filled.min(bar_w);
            let secs = rem.as_millis() as f64 / 1000.0;
            Line::from(vec![
                Span::styled(
                    "█".repeat(filled),
                    Style::default().fg(theme.status_waiting),
                ),
                Span::styled(
                    "░".repeat(bar_w - filled),
                    Style::default().fg(theme.border_inactive),
                ),
                Span::styled(
                    format!(" next in {secs:.1}s"),
                    Style::default().fg(theme.text_muted),
                ),
            ])
        }
        _ => Line::from(Span::styled(
            "paused, pick one",
            Style::default().fg(theme.text_muted),
        )),
    };
    let hints = Line::from(Span::styled(
        "1-9 jump   j/k move   ⏎ go   n next   q quit",
        Style::default().fg(theme.text_inactive),
    ));
    Paragraph::new(vec![bar, hints])
}
