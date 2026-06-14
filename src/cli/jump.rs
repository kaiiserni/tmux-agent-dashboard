//! `jump`: a small `display-popup` picker with two tabs.
//!
//! `prefix + n` opens it. The **Jump** tab lists the pending agents (the
//! same panes as the bottom status bar), numbered, with a countdown: leave
//! it alone and it jumps to the most urgent one (the old `prefix + n`
//! behaviour); press a digit, or navigate and confirm, to pick another.
//!
//! `/` or `Tab` switches to the **Search** tab: a fuzzy filter over *every*
//! agent pane across all sessions. It follows vim modality — arriving on the
//! tab drops you in insert mode (type to filter); `Esc` returns to normal
//! mode (`j`/`k` to move); `Esc`/`Tab` again goes back to the Jump tab.

use std::io;
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Padding, Paragraph},
};

use crate::fuzzy;
use crate::navigate;
use crate::pending::{self, PendingEntry, Priority};
use crate::tmux::{self, PaneStatus};
use crate::ui::colors::ColorTheme;

const JUMP_TIMEOUT_MS: &str = "@dashboard_jump_timeout_ms";
const DEFAULT_TIMEOUT_MS: u64 = 250;

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

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Jump,
    Search,
}

enum Outcome {
    Jump(String),
    Cancel,
}

/// Fuzzy haystack for an entry: repo/session and label/branch. The agent
/// vendor name is excluded — it's identical across most panes ("claude") so
/// it would match every short query.
fn haystack(e: &PendingEntry) -> String {
    format!("{}/{}", e.repo, e.label)
}

/// Indices into `all`, kept only when they match `query`. The fuzzy score
/// only decides membership; ordering is either newest-activity-first or
/// alphabetical (repo/label), toggled with `s`. Empty query keeps everything.
fn filter_all(all: &[PendingEntry], query: &str, sort_alpha: bool) -> Vec<usize> {
    let mut idx: Vec<usize> = all
        .iter()
        .enumerate()
        .filter(|(_, e)| fuzzy::score(&haystack(e), query).is_some())
        .map(|(i, _)| i)
        .collect();
    if sort_alpha {
        idx.sort_by(|&a, &b| {
            let ka = (all[a].repo.to_lowercase(), all[a].label.to_lowercase());
            let kb = (all[b].repo.to_lowercase(), all[b].label.to_lowercase());
            ka.cmp(&kb)
        });
    } else {
        idx.sort_by(|&a, &b| all[b].mtime.cmp(&all[a].mtime));
    }
    idx
}

pub fn cmd_jump(start_search: bool) -> io::Result<()> {
    let entries = pending::collect_pending();
    // One pending candidate: skip the popup, just go there — unless the
    // caller explicitly asked to open in search mode.
    if !start_search && entries.len() == 1 {
        navigate::jump_to(&entries[0].pane_id);
        return Ok(());
    }
    cmd_jump_inner(entries, start_search)
}

fn cmd_jump_inner(mut entries: Vec<PendingEntry>, start_search: bool) -> io::Result<()> {
    let timeout_ms = tmux::get_option(JUMP_TIMEOUT_MS)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_TIMEOUT_MS);

    let theme = ColorTheme::from_tmux();

    let mut stdout = io::stdout();
    let _guard = JumpTui::enter(&mut stdout)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut selected = 0usize;
    let mut query = String::new();
    // Search list ordering: false = newest activity first, true = a-z.
    let mut sort_alpha = false;
    // Search data, populated lazily the first time the Search tab is opened.
    let mut all: Vec<PendingEntry> = Vec::new();
    let mut filtered: Vec<usize> = Vec::new();

    // Open on the Search tab when asked, or when there are no pending
    // agents (the Jump list would be empty).
    let mut tab = if start_search || entries.is_empty() {
        Tab::Search
    } else {
        Tab::Jump
    };
    // The tab the popup opened on: Esc backs out toward closing relative to
    // this, so `prefix s` (home = Search) closes on Esc instead of dropping
    // into a Jump tab the user never came from.
    let home = tab;
    let mut insert = tab == Tab::Search;
    if tab == Tab::Search {
        all = pending::collect_all();
        filtered = filter_all(&all, &query, sort_alpha);
    }

    // Countdown only runs on the Jump tab. Starts on the most urgent entry,
    // so an untouched popup == the old `prefix + n`.
    let mut countdown = if timeout_ms > 0 && tab == Tab::Jump && !entries.is_empty() {
        Some((Instant::now(), Duration::from_millis(timeout_ms)))
    } else {
        None
    };

    let outcome = loop {
        let count = match tab {
            Tab::Jump => entries.len(),
            Tab::Search => filtered.len(),
        };
        if count == 0 {
            selected = 0;
        } else if selected >= count {
            selected = count - 1;
        }

        // Countdown expiry → jump to the highlighted pending entry.
        if tab == Tab::Jump
            && let Some((start, dur)) = countdown
            && start.elapsed() >= dur
            && !entries.is_empty()
        {
            break Outcome::Jump(entries[selected].pane_id.clone());
        }

        let remaining = if tab == Tab::Jump {
            countdown.map(|(start, dur)| dur.saturating_sub(start.elapsed()))
        } else {
            None
        };

        {
            let view: Vec<&PendingEntry> = match tab {
                Tab::Jump => entries.iter().collect(),
                Tab::Search => filtered.iter().map(|&i| &all[i]).collect(),
            };
            terminal.draw(|f| {
                render(
                    f, tab, insert, sort_alpha, &view, selected, &query, remaining, timeout_ms,
                    &theme,
                );
            })?;
        }

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
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        countdown = None;

        match tab {
            Tab::Jump => match k.code {
                KeyCode::Char('c') if ctrl => break Outcome::Cancel,
                KeyCode::Esc | KeyCode::Char('q') => break Outcome::Cancel,
                KeyCode::Tab | KeyCode::Char('/') | KeyCode::Char('s') => {
                    if all.is_empty() {
                        all = pending::collect_all();
                    }
                    filtered = filter_all(&all, &query, sort_alpha);
                    tab = Tab::Search;
                    insert = true;
                    selected = 0;
                }
                KeyCode::Enter => {
                    if let Some(e) = entries.get(selected) {
                        break Outcome::Jump(e.pane_id.clone());
                    }
                }
                KeyCode::Char('n') => {
                    if !entries.is_empty() {
                        break Outcome::Jump(next_target(&entries).to_string());
                    }
                }
                // `o`: flip the pending order (status-line bar + this list).
                // Marked-unread stays at the back. Persisted globally.
                KeyCode::Char('o') => {
                    let reversed = tmux::get_option(tmux::DASHBOARD_PENDING_REVERSE)
                        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false);
                    tmux::set_global_option(
                        tmux::DASHBOARD_PENDING_REVERSE,
                        if reversed { "0" } else { "1" },
                    );
                    entries = pending::collect_pending();
                    selected = 0;
                    countdown = None;
                    tmux::refresh_status();
                }
                KeyCode::Char(c @ '1'..='9') => {
                    let idx = (c as u8 - b'1') as usize;
                    if idx < entries.len() {
                        break Outcome::Jump(entries[idx].pane_id.clone());
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if selected + 1 < count {
                        selected += 1;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
                KeyCode::Char('g') | KeyCode::Home => selected = 0,
                KeyCode::Char('G') | KeyCode::End => selected = count.saturating_sub(1),
                _ => {}
            },
            Tab::Search if insert => match k.code {
                KeyCode::Char('c') if ctrl => break Outcome::Cancel,
                // Esc / Tab leave insert for normal mode (Tab cycles
                // Jump → insert → normal → Jump).
                KeyCode::Esc | KeyCode::Tab => insert = false,
                KeyCode::Enter => {
                    if let Some(&i) = filtered.get(selected) {
                        break Outcome::Jump(all[i].pane_id.clone());
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('j') if ctrl => {
                    if selected + 1 < count {
                        selected += 1;
                    }
                }
                KeyCode::Char('p') | KeyCode::Char('k') if ctrl => {
                    selected = selected.saturating_sub(1)
                }
                KeyCode::Down => {
                    if selected + 1 < count {
                        selected += 1;
                    }
                }
                KeyCode::Up => selected = selected.saturating_sub(1),
                KeyCode::Backspace => {
                    query.pop();
                    filtered = filter_all(&all, &query, sort_alpha);
                    selected = 0;
                }
                KeyCode::Char(c) if !c.is_control() => {
                    query.push(c);
                    filtered = filter_all(&all, &query, sort_alpha);
                    selected = 0;
                }
                _ => {}
            },
            Tab::Search => match k.code {
                KeyCode::Char('c') if ctrl => break Outcome::Cancel,
                KeyCode::Esc => {
                    if home == Tab::Search {
                        break Outcome::Cancel;
                    }
                    tab = Tab::Jump;
                    selected = 0;
                }
                KeyCode::Tab => {
                    tab = Tab::Jump;
                    selected = 0;
                }
                KeyCode::Char('q') => break Outcome::Cancel,
                KeyCode::Char('s') => {
                    sort_alpha = !sort_alpha;
                    filtered = filter_all(&all, &query, sort_alpha);
                    selected = 0;
                }
                KeyCode::Char('i') | KeyCode::Char('/') => insert = true,
                KeyCode::Enter => {
                    if let Some(&i) = filtered.get(selected) {
                        break Outcome::Jump(all[i].pane_id.clone());
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if selected + 1 < count {
                        selected += 1;
                    }
                }
                KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
                KeyCode::Char('g') | KeyCode::Home => selected = 0,
                KeyCode::Char('G') | KeyCode::End => selected = count.saturating_sub(1),
                _ => {}
            },
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
        .or_else(|| entries.first())
        .map(|e| e.pane_id.as_str())
        .unwrap_or("")
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

/// Glyph for the search list, where most rows aren't "pending": fall back to
/// a status-derived marker for plain running/idle panes.
fn entry_glyph(e: &PendingEntry) -> (&'static str, u8) {
    match e.priority {
        Priority::MarkedUnread => match e.status {
            PaneStatus::Running => ("●", 109),
            PaneStatus::Background => ("◌", 109),
            PaneStatus::Idle => ("·", 244),
            _ => priority_glyph(e.priority),
        },
        other => priority_glyph(other),
    }
}

fn entry_line(
    e: &PendingEntry,
    prefix: Span<'static>,
    theme: &ColorTheme,
    glyph: (&'static str, u8),
) -> ListItem<'static> {
    use ratatui::style::Color;
    let (icon, icon_color) = glyph;
    let mut spans = vec![
        prefix,
        Span::styled(icon, Style::default().fg(Color::Indexed(icon_color))),
        Span::raw(" "),
        Span::styled(e.agent.glyph(), Style::default().fg(theme.agent_color(&e.agent))),
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
}

#[allow(clippy::too_many_arguments)]
fn render(
    f: &mut ratatui::Frame,
    tab: Tab,
    insert: bool,
    sort_alpha: bool,
    view: &[&PendingEntry],
    selected: usize,
    query: &str,
    remaining: Option<Duration>,
    timeout_ms: u64,
    theme: &ColorTheme,
) {
    let searching = tab == Tab::Search;

    let area = f.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .padding(Padding::horizontal(1))
        .title(Span::styled(
            " Agents ",
            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let constraints = if searching {
        vec![
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ]
    } else {
        vec![Constraint::Length(1), Constraint::Min(1), Constraint::Length(2)]
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    f.render_widget(tabs_line(tab, theme), rows[0]);

    let (list_area, footer_area): (Rect, Rect) = if searching {
        f.render_widget(search_input(query, insert, theme), rows[1]);
        (rows[2], rows[3])
    } else {
        (rows[1], rows[2])
    };

    let items: Vec<ListItem> = view
        .iter()
        .enumerate()
        .map(|(i, e)| {
            if searching {
                entry_line(e, Span::raw(""), theme, entry_glyph(e))
            } else {
                let num = if i < 9 {
                    format!("{} ", i + 1)
                } else {
                    "· ".to_string()
                };
                let prefix = Span::styled(num, Style::default().fg(theme.text_inactive));
                entry_line(e, prefix, theme, priority_glyph(e.priority))
            }
        })
        .collect();

    let list = List::new(items).highlight_symbol("▌ ").highlight_style(
        Style::default()
            .bg(theme.selection_bg)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    if !view.is_empty() {
        state.select(Some(selected));
    }
    f.render_stateful_widget(list, list_area, &mut state);

    let footer = if searching {
        let sort = if sort_alpha { "a-z" } else { "recent" };
        let hint = if view.is_empty() {
            "no matches   esc/⇥ jump tab".to_string()
        } else if insert {
            format!("type to filter   ^j/^k move   ⏎ go   esc/⇥ normal   [{sort}]")
        } else {
            format!("j/k move   s sort   ⏎ go   i // search   esc/⇥ jump   [{sort}]")
        };
        Paragraph::new(Line::from(Span::styled(
            hint,
            Style::default().fg(theme.text_inactive),
        )))
    } else {
        jump_footer(remaining, timeout_ms, footer_area.width as usize, theme)
    };
    f.render_widget(footer, footer_area);
}

fn tabs_line(tab: Tab, theme: &ColorTheme) -> Paragraph<'static> {
    let chip = |label: &'static str, active: bool| {
        if active {
            Span::styled(
                format!(" {label} "),
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(format!(" {label} "), Style::default().fg(theme.text_inactive))
        }
    };
    Paragraph::new(Line::from(vec![
        chip("Jump", tab == Tab::Jump),
        Span::raw(" "),
        chip("Search", tab == Tab::Search),
    ]))
}

fn search_input(query: &str, insert: bool, theme: &ColorTheme) -> Paragraph<'static> {
    let cursor = if insert { "▏" } else { "" };
    Paragraph::new(Line::from(vec![
        Span::styled("/", Style::default().fg(theme.accent)),
        Span::styled(query.to_string(), Style::default().fg(theme.text_active)),
        Span::styled(cursor, Style::default().fg(theme.accent)),
    ]))
}

fn jump_footer(
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
                Span::styled("█".repeat(filled), Style::default().fg(theme.status_waiting)),
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
        "1-9 jump   j/k move   ⏎ go   n next   o order   s // search   q quit",
        Style::default().fg(theme.text_inactive),
    ));
    Paragraph::new(vec![bar, hints])
}
