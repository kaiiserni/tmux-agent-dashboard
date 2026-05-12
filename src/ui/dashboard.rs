//! Dashboard layout rendered inside a `tmux display-popup`.
//!
//! Four quadrants: aggregate counters on top, attention-list (left) and
//! per-repo summary (right) in the middle, and a global recent-activity
//! feed at the bottom. Reuses [`AppState`], [`ColorTheme`], [`StatusIcons`]
//! and the activity parser; only adds layout + a few aggregation helpers.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::activity::{GlobalActivityEntry, read_all_activity};
use crate::state::{AppState, DashboardTab, SummarySection};
use crate::tmux::{PaneInfo, PaneStatus};

use super::text::truncate_to_width;

const ACTIVITY_MAX_ENTRIES: usize = 200;

pub fn draw_dashboard(frame: &mut Frame, state: &mut AppState) {
    let area = frame.area();

    let tab_label = match state.dashboard_tab {
        DashboardTab::Summary => " Summary ",
        DashboardTab::Tiles => " Tiles ",
    };
    let outer = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            format!(" Agents Dashboard ·{tab_label}· Tab: switch · q: close "),
            Style::default()
                .fg(state.theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().fg(state.theme.border_inactive));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    match state.dashboard_tab {
        DashboardTab::Summary => draw_summary(frame, state, inner),
        DashboardTab::Tiles => draw_tiles(frame, state, inner),
    }
}

fn draw_summary(frame: &mut Frame, state: &mut AppState, area: Rect) {
    state.layout.summary_targets.clear();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // counters
            Constraint::Min(10),   // [attention / waitings] | per-repo
            Constraint::Length(9), // activity
        ])
        .split(area);

    draw_counters(frame, state, chunks[0]);

    // Middle row: left column stacks attention (top) + waitings (bottom);
    // right column is per-repo spanning the full middle height.
    let middle = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[1]);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(middle[0]);
    draw_attention(frame, state, left[0]);
    draw_waiting(frame, state, left[1]);
    draw_responded(frame, state, left[2]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(middle[1]);
    draw_running(frame, state, right[0]);
    draw_marked_unread(frame, state, right[1]);
    draw_idle(frame, state, right[2]);

    // Clamp selection across the combined summary_targets after both lists rendered.
    let total = state.layout.summary_targets.len();
    if total > 0 && state.summary_selected >= total {
        state.summary_selected = total - 1;
    }

    draw_activity(frame, state, chunks[2]);
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn block_with_title(state: &AppState, title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            format!(" {} ", title_case(title)),
            Style::default()
                .fg(state.theme.section_title)
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().fg(state.theme.border_inactive))
}

fn draw_counters(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let block = block_with_title(state, "overview");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (all, running, background, waiting, idle, error) = state.status_counts();
    let attention = count_attention(state);

    let make = |icon: &str, label: &str, n: usize, color: ratatui::style::Color| {
        vec![
            Span::styled(format!("{icon} "), Style::default().fg(color)),
            Span::styled(
                format!("{n:>3}"),
                Style::default()
                    .fg(state.theme.text_active)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {label}"),
                Style::default().fg(state.theme.text_muted),
            ),
            Span::raw("   "),
        ]
    };

    let mut spans: Vec<Span> = Vec::new();
    spans.extend(make(
        state.icons.all_icon(),
        "all",
        all,
        state.theme.status_all,
    ));
    spans.extend(make(
        state.icons.status_icon(&PaneStatus::Running),
        "running",
        running,
        state.theme.status_running,
    ));
    spans.extend(make(
        state.icons.status_icon(&PaneStatus::Background),
        "bg",
        background,
        state.theme.status_running,
    ));
    spans.extend(make(
        state.icons.status_icon(&PaneStatus::Waiting),
        "waiting",
        waiting,
        state.theme.status_waiting,
    ));
    spans.extend(make(
        state.icons.status_icon(&PaneStatus::Idle),
        "idle",
        idle,
        state.theme.status_idle,
    ));
    spans.extend(make(
        state.icons.status_icon(&PaneStatus::Error),
        "error",
        error,
        state.theme.status_error,
    ));
    spans.push(Span::styled(
        "▲ ",
        Style::default().fg(state.theme.badge_danger),
    ));
    spans.push(Span::styled(
        format!("{attention:>3}"),
        Style::default()
            .fg(state.theme.text_active)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        " needs attention",
        Style::default().fg(state.theme.text_muted),
    ));

    let para = Paragraph::new(Line::from(spans));
    frame.render_widget(para, inner);
}

fn count_attention(state: &AppState) -> usize {
    state
        .repo_groups
        .iter()
        .flat_map(|g| g.panes.iter())
        .filter(|(p, _)| needs_attention(&p.status, p.attention))
        .count()
}

fn needs_attention(status: &PaneStatus, attention: bool) -> bool {
    attention || matches!(status, PaneStatus::Waiting | PaneStatus::Error)
}

fn draw_attention(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let block = block_with_title(state, "needs attention");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Collect attention-flagged panes (notification / permission_denied / teammate_idle).
    let rows: Vec<SummaryRow> = state
        .repo_groups
        .iter()
        .flat_map(|g| {
            g.panes
                .iter()
                .map(move |(p, info)| (g.name.clone(), p, info))
        })
        .filter(|(_, p, _)| p.attention)
        .map(|(group_name, p, info)| {
            let branch = resolve_branch(p, info);
            SummaryRow {
                status_icon: state.icons.status_icon(&p.status).to_string(),
                status_color: state.theme.badge_danger,
                agent_glyph: p.agent.glyph(),
                agent_color: agent_color(state, &p.agent),
                title: format!("{group_name}  {branch}"),
                reason: if !p.wait_reason.is_empty() {
                    p.wait_reason.clone()
                } else {
                    "notification".into()
                },
                pane_id: p.pane_id.clone(),
            }
        })
        .collect();

    if rows.is_empty() {
        update_section_rect(state, SummarySection::Attention, inner, 0);
        let para = Paragraph::new(Line::from(Span::styled(
            "  all clear ✓",
            Style::default().fg(state.theme.status_idle),
        )));
        frame.render_widget(para, inner);
        return;
    }

    render_list_with_targets(frame, state, inner, &rows, SummarySection::Attention);
}

fn draw_waiting(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let block = block_with_title(state, "waiting");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows: Vec<SummaryRow> = state
        .repo_groups
        .iter()
        .flat_map(|g| {
            g.panes
                .iter()
                .map(move |(p, info)| (g.name.clone(), p, info))
        })
        .filter(|(_, p, _)| matches!(p.status, PaneStatus::Waiting | PaneStatus::Error))
        .map(|(group_name, p, info)| {
            let branch = resolve_branch(p, info);
            SummaryRow {
                status_icon: state.icons.status_icon(&p.status).to_string(),
                status_color: if matches!(p.status, PaneStatus::Error) {
                    state.theme.status_error
                } else {
                    state.theme.status_waiting
                },
                agent_glyph: p.agent.glyph(),
                agent_color: agent_color(state, &p.agent),
                title: format!("{group_name}  {branch}"),
                reason: if !p.wait_reason.is_empty() {
                    p.wait_reason.clone()
                } else if matches!(p.status, PaneStatus::Error) {
                    "error".into()
                } else {
                    "waiting".into()
                },
                pane_id: p.pane_id.clone(),
            }
        })
        .collect();

    if rows.is_empty() {
        update_section_rect(state, SummarySection::Waiting, inner, 0);
        let para = Paragraph::new(Line::from(Span::styled(
            "  no agents waiting",
            Style::default().fg(state.theme.text_muted),
        )));
        frame.render_widget(para, inner);
        return;
    }

    render_list_with_targets(frame, state, inner, &rows, SummarySection::Waiting);
}

fn draw_running(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let block = block_with_title(state, "running");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows: Vec<SummaryRow> = state
        .repo_groups
        .iter()
        .flat_map(|g| {
            g.panes
                .iter()
                .map(move |(p, info)| (g.name.clone(), p, info))
        })
        .filter(|(_, p, _)| matches!(p.status, PaneStatus::Running | PaneStatus::Background))
        .map(|(group_name, p, info)| {
            let branch = resolve_branch(p, info);
            SummaryRow {
                status_icon: state.icons.status_icon(&p.status).to_string(),
                status_color: state.theme.status_running,
                agent_glyph: p.agent.glyph(),
                agent_color: agent_color(state, &p.agent),
                title: format!("{group_name}  {branch}"),
                reason: if !p.prompt.is_empty() {
                    p.prompt.clone()
                } else if !p.current_command.is_empty() {
                    p.current_command.clone()
                } else if matches!(p.status, PaneStatus::Background) {
                    "background".into()
                } else {
                    "running".into()
                },
                pane_id: p.pane_id.clone(),
            }
        })
        .collect();

    if rows.is_empty() {
        update_section_rect(state, SummarySection::Running, inner, 0);
        let para = Paragraph::new(Line::from(Span::styled(
            "  no running agents",
            Style::default().fg(state.theme.text_muted),
        )));
        frame.render_widget(para, inner);
        return;
    }

    render_list_with_targets(frame, state, inner, &rows, SummarySection::Running);
}

/// Best human-readable label for a pane row. Prefers the Claude session
/// name (set via `/rename`) since it's the user's chosen identity; falls
/// back to the resolved git branch and other progressively less specific
/// signals.
fn resolve_branch(pane: &PaneInfo, info: &crate::group::PaneGitInfo) -> String {
    if !pane.session_name.is_empty() {
        return pane.session_name.clone();
    }
    if let Some(b) = info.branch.as_ref().filter(|s| !s.is_empty()) {
        return b.clone();
    }
    if !pane.worktree.branch.is_empty() {
        return pane.worktree.branch.clone();
    }
    "-".into()
}

struct SummaryRow {
    status_icon: String,
    status_color: ratatui::style::Color,
    agent_glyph: &'static str,
    agent_color: ratatui::style::Color,
    title: String,
    reason: String,
    pane_id: String,
}

/// Render a vertical list (status icon · agent glyph · title · reason) and
/// append one `SummaryTarget` per row so the same rows are clickable /
/// navigable. `section` identifies which summary list this is so input
/// handlers can scroll the right one.
fn render_list_with_targets(
    frame: &mut Frame,
    state: &mut AppState,
    inner: Rect,
    rows: &[SummaryRow],
    section: SummarySection,
) {
    let total = rows.len();
    let max_rows = (inner.height as usize).max(1);
    // Clamp scroll offset to a value that keeps the last rows visible.
    let max_scroll = total.saturating_sub(max_rows);
    let scroll = section_scroll(state, section).min(max_scroll);
    set_section_scroll(state, section, scroll);

    update_section_rect(state, section, inner, total);

    let width = inner.width as usize;
    let visible = rows.iter().skip(scroll).take(max_rows);

    for (i, row) in visible.enumerate() {
        let target_idx = state.layout.summary_targets.len();
        let selected = target_idx == state.summary_selected;
        let row_rect = Rect {
            x: inner.x,
            y: inner.y + i as u16,
            width: inner.width,
            height: 1,
        };
        state
            .layout
            .summary_targets
            .push(crate::state::SummaryTarget {
                rect: row_rect,
                pane_id: row.pane_id.clone(),
                section,
            });

        let prefix = if selected { "▌ " } else { "  " };
        // Reserve a fixed slice of the row for the title so a long reason
        // can never visually erase the identity column.
        const FIXED_OVERHEAD: usize = 2 /* prefix */ + 2 /* icon + space */ + 2 /* glyph + space */;
        const TITLE_MAX: usize = 28;
        let available = width.saturating_sub(FIXED_OVERHEAD);
        let title_w = available.min(TITLE_MAX);
        let reason_w = available.saturating_sub(title_w + 3); // "  (" prefix
        let title_trim = truncate_to_width(&row.title, title_w);
        let reason_trim = truncate_to_width(&row.reason, reason_w);
        let title_padded = format!("{title_trim:<title_w$}");
        let title_style = Style::default()
            .fg(state.theme.text_active)
            .add_modifier(if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
        let prefix_style = Style::default().fg(if selected {
            state.theme.accent
        } else {
            state.theme.border_inactive
        });
        let line = Line::from(vec![
            Span::styled(prefix.to_string(), prefix_style),
            Span::styled(
                format!("{} ", row.status_icon),
                Style::default().fg(row.status_color),
            ),
            Span::styled(
                format!("{} ", row.agent_glyph),
                Style::default().fg(row.agent_color),
            ),
            Span::styled(title_padded, title_style),
            Span::styled(
                format!("  ({reason_trim})"),
                Style::default().fg(state.theme.wait_reason),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), row_rect);
    }
}

fn section_scroll(state: &AppState, section: SummarySection) -> usize {
    match section {
        SummarySection::Attention => state.summary_scroll_attention,
        SummarySection::Waiting => state.summary_scroll_waiting,
        SummarySection::Responded => state.summary_scroll_responded,
        SummarySection::Running => state.summary_scroll_running,
        SummarySection::MarkedUnread => state.summary_scroll_marked_unread,
        SummarySection::Idle => state.summary_scroll_idle,
    }
}

fn set_section_scroll(state: &mut AppState, section: SummarySection, value: usize) {
    match section {
        SummarySection::Attention => state.summary_scroll_attention = value,
        SummarySection::Waiting => state.summary_scroll_waiting = value,
        SummarySection::Responded => state.summary_scroll_responded = value,
        SummarySection::Running => state.summary_scroll_running = value,
        SummarySection::MarkedUnread => state.summary_scroll_marked_unread = value,
        SummarySection::Idle => state.summary_scroll_idle = value,
    }
}

fn update_section_rect(state: &mut AppState, section: SummarySection, rect: Rect, total: usize) {
    let entry = match section {
        SummarySection::Attention => &mut state.layout.summary_section_attention,
        SummarySection::Waiting => &mut state.layout.summary_section_waiting,
        SummarySection::Responded => &mut state.layout.summary_section_responded,
        SummarySection::Running => &mut state.layout.summary_section_running,
        SummarySection::MarkedUnread => &mut state.layout.summary_section_marked_unread,
        SummarySection::Idle => &mut state.layout.summary_section_idle,
    };
    entry.rect = rect;
    entry.total_rows = total;
}

/// Heuristic: "agent has a reply the user hasn't seen yet".
/// Compares the activity log file's mtime (which the Stop hook bumps
/// when it writes the `__task_reset__` marker — and any other entry)
/// against the epoch timestamp written to `@pane_last_seen_at` by the
/// `seen` CLI subcommand on focus change. If the log was touched after
/// the user last looked, the pane lands in the Responded list.
fn pane_is_unseen(pane: &crate::tmux::PaneInfo) -> bool {
    let log_mtime = match crate::activity::log_mtime(&pane.pane_id) {
        Some(m) => m,
        None => return false,
    };
    let log_secs = log_mtime
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    match pane.last_seen_at {
        None => true,
        Some(seen) => log_secs > seen,
    }
}

fn agent_color(state: &AppState, agent: &crate::tmux::AgentType) -> ratatui::style::Color {
    match agent {
        crate::tmux::AgentType::Claude => state.theme.agent_claude,
        crate::tmux::AgentType::Codex => state.theme.agent_codex,
        crate::tmux::AgentType::OpenCode => state.theme.agent_opencode,
        crate::tmux::AgentType::Unknown => state.theme.text_muted,
    }
}

fn draw_responded(frame: &mut Frame, state: &mut AppState, area: Rect) {
    // Tilde signals "this is a heuristic" — derived from activity log
    // mtime vs the user's last focus time; we can't actually know whether
    // the user has read the reply.
    let block = block_with_title(state, "~ responded");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Idle agents whose activity log was touched after the user last
    // focused the pane (= unseen reply). Running panes naturally bump
    // the log too, so we restrict to status == Idle to avoid overlap
    // with the Running list. Sorted newest-first by mtime.
    let mut rows: Vec<(SummaryRow, std::time::SystemTime)> = state
        .repo_groups
        .iter()
        .flat_map(|g| {
            g.panes
                .iter()
                .map(move |(p, info)| (g.name.clone(), p, info))
        })
        .filter(|(_, p, _)| matches!(p.status, PaneStatus::Idle) && pane_is_unseen(p))
        .map(|(group_name, p, info)| {
            let branch = resolve_branch(p, info);
            let mtime = crate::activity::log_mtime(&p.pane_id).unwrap_or(std::time::UNIX_EPOCH);
            let last_entry = crate::activity::read_activity_log(&p.pane_id, 1)
                .into_iter()
                .next();
            let reason = if !p.prompt.is_empty() {
                p.prompt.clone()
            } else if let Some(e) = last_entry {
                if e.label.is_empty() {
                    format!("{} {}", e.timestamp, e.tool)
                } else {
                    format!("{} {} {}", e.timestamp, e.tool, e.label)
                }
            } else {
                "responded".into()
            };
            let row = SummaryRow {
                status_icon: "↩".to_string(),
                status_color: state.theme.badge_auto,
                agent_glyph: p.agent.glyph(),
                agent_color: agent_color(state, &p.agent),
                title: format!("{group_name}  {branch}"),
                reason,
                pane_id: p.pane_id.clone(),
            };
            (row, mtime)
        })
        .collect();

    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let rows: Vec<SummaryRow> = rows.into_iter().map(|(r, _)| r).collect();

    if rows.is_empty() {
        update_section_rect(state, SummarySection::Responded, inner, 0);
        let para = Paragraph::new(Line::from(Span::styled(
            "  no unseen replies",
            Style::default().fg(state.theme.text_muted),
        )));
        frame.render_widget(para, inner);
        return;
    }

    render_list_with_targets(frame, state, inner, &rows, SummarySection::Responded);
}

fn draw_marked_unread(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let block = block_with_title(state, "marked unread");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Pinned-by-user panes that are still purely Idle (the cleanup sweep
    // in state.refresh() drops marks the moment a pane gets busy again).
    // Sorted newest-marked first.
    let mut rows: Vec<(SummaryRow, u64)> = state
        .repo_groups
        .iter()
        .flat_map(|g| {
            g.panes
                .iter()
                .map(move |(p, info)| (g.name.clone(), p, info))
        })
        .filter(|(_, p, _)| {
            matches!(p.status, PaneStatus::Idle)
                && !p.attention
                && p.marked_unread_at.is_some()
                && !crate::pending::pane_is_unseen(p)
        })
        .map(|(group_name, p, info)| {
            let branch = resolve_branch(p, info);
            let marked_at = p.marked_unread_at.unwrap_or(0);
            let reason = if !p.prompt.is_empty() {
                p.prompt.clone()
            } else {
                "pinned".into()
            };
            let row = SummaryRow {
                status_icon: "📌".to_string(),
                status_color: state.theme.badge_plan,
                agent_glyph: p.agent.glyph(),
                agent_color: agent_color(state, &p.agent),
                title: format!("{group_name}  {branch}"),
                reason,
                pane_id: p.pane_id.clone(),
            };
            (row, marked_at)
        })
        .collect();

    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let rows: Vec<SummaryRow> = rows.into_iter().map(|(r, _)| r).collect();

    if rows.is_empty() {
        update_section_rect(state, SummarySection::MarkedUnread, inner, 0);
        let para = Paragraph::new(Line::from(Span::styled(
            "  no pinned panes",
            Style::default().fg(state.theme.text_muted),
        )));
        frame.render_widget(para, inner);
        return;
    }

    render_list_with_targets(frame, state, inner, &rows, SummarySection::MarkedUnread);
}

fn draw_idle(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let block = block_with_title(state, "idle");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Idle panes the user has already viewed. Unseen replies appear in
    // the Responded list instead.
    let mut rows: Vec<(SummaryRow, std::time::SystemTime)> = state
        .repo_groups
        .iter()
        .flat_map(|g| {
            g.panes
                .iter()
                .map(move |(p, info)| (g.name.clone(), p, info))
        })
        .filter(|(_, p, _)| {
            matches!(p.status, PaneStatus::Idle)
                && !pane_is_unseen(p)
                && p.marked_unread_at.is_none()
        })
        .map(|(group_name, p, info)| {
            let branch = resolve_branch(p, info);
            let mtime = crate::activity::log_mtime(&p.pane_id).unwrap_or(std::time::UNIX_EPOCH);
            // Prefer the last assistant message / user prompt (same source
            // Running and Responded use). Fall back to the latest activity
            // log entry, then to a static "idle" label.
            let reason = if !p.prompt.is_empty() {
                p.prompt.clone()
            } else if let Some(e) = crate::activity::read_activity_log(&p.pane_id, 1)
                .into_iter()
                .next()
            {
                if e.label.is_empty() {
                    format!("{} {}", e.timestamp, e.tool)
                } else {
                    format!("{} {} {}", e.timestamp, e.tool, e.label)
                }
            } else {
                "idle".into()
            };
            let row = SummaryRow {
                status_icon: state.icons.status_icon(&p.status).to_string(),
                status_color: state.theme.status_idle,
                agent_glyph: p.agent.glyph(),
                agent_color: agent_color(state, &p.agent),
                title: format!("{group_name}  {branch}"),
                reason,
                pane_id: p.pane_id.clone(),
            };
            (row, mtime)
        })
        .collect();

    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let rows: Vec<SummaryRow> = rows.into_iter().map(|(r, _)| r).collect();

    if rows.is_empty() {
        update_section_rect(state, SummarySection::Idle, inner, 0);
        let para = Paragraph::new(Line::from(Span::styled(
            "  no idle agents",
            Style::default().fg(state.theme.text_muted),
        )));
        frame.render_widget(para, inner);
        return;
    }

    render_list_with_targets(frame, state, inner, &rows, SummarySection::Idle);
}

fn draw_activity(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let block = block_with_title(state, "recent activity");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let merged = read_all_activity(ACTIVITY_MAX_ENTRIES);
    if merged.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "  no activity yet",
            Style::default().fg(state.theme.text_muted),
        )));
        frame.render_widget(para, inner);
        return;
    }

    let lines: Vec<Line> = merged
        .iter()
        .take(inner.height as usize)
        .map(|e| format_activity_line(state, e, inner.width as usize))
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn format_activity_line<'a>(
    state: &'a AppState,
    entry: &'a GlobalActivityEntry,
    width: usize,
) -> Line<'a> {
    let repo_label = resolve_repo_label(state, &entry.pane_id);
    let tool_color = ratatui::style::Color::Indexed(entry.entry.tool_color_index());
    let label = truncate_to_width(&entry.entry.label, width.saturating_sub(40));
    Line::from(vec![
        Span::styled(
            format!(" {} ", entry.entry.timestamp),
            Style::default().fg(state.theme.activity_timestamp),
        ),
        Span::styled(
            format!("{:<18}", truncate_to_width(&repo_label, 18)),
            Style::default().fg(state.theme.session_header),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{:<14}", truncate_to_width(&entry.entry.tool, 14)),
            Style::default().fg(tool_color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(label, Style::default().fg(state.theme.text_muted)),
    ])
}

fn resolve_repo_label(state: &AppState, pane_id: &str) -> String {
    for group in &state.repo_groups {
        for (pane, _) in &group.panes {
            if pane.pane_id == pane_id {
                return group.name.clone();
            }
        }
    }
    "—".into()
}

// ── tiles view (accordion) ──────────────────────────────────────────
//
// Vertical list, one line per pane (collapsed) and a few extra lines
// for the currently-selected pane (expanded inline body with `│` bar).
// Group headers separate panes by repo. Auto-scroll keeps the
// selected pane in view.

const GROUP_HEADER_H: u16 = 1;
/// Total height of an expanded pane (1 summary + 2 body lines).
const EXPANDED_H: u16 = 3;

#[derive(Clone, Copy)]
struct TileEntry {
    group_idx: usize,
    pane_idx: usize,
}

fn draw_tiles(frame: &mut Frame, state: &mut AppState, area: Rect) {
    state.layout.tile_targets.clear();
    state.layout.tile_cols = 0;
    state.layout.tile_visible_first = 0;
    state.layout.tile_visible_last = 0;

    // Flat list of pane entries across groups (excludes empty groups).
    let mut entries: Vec<TileEntry> = Vec::new();
    for (g_idx, group) in state.repo_groups.iter().enumerate() {
        for p_idx in 0..group.panes.len() {
            entries.push(TileEntry {
                group_idx: g_idx,
                pane_idx: p_idx,
            });
        }
    }

    if entries.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "  no agents to show",
            Style::default().fg(state.theme.text_muted),
        )));
        frame.render_widget(para, area);
        return;
    }

    // Clamp selection to the visible tile range.
    if state.tile_selected >= entries.len() {
        state.tile_selected = entries.len() - 1;
    }
    let selected = state.tile_selected;

    // Compute y-position + height for each entry, and y-position for
    // each group header. Heights are in rows starting from 0 (virtual
    // canvas — scroll offset is applied at render time).
    let mut entry_pos: Vec<(u16, u16)> = Vec::with_capacity(entries.len());
    let mut header_pos: Vec<(usize, u16)> = Vec::new();
    let mut y: u16 = 0;
    let mut last_group: Option<usize> = None;
    for (i, e) in entries.iter().enumerate() {
        if last_group != Some(e.group_idx) {
            header_pos.push((e.group_idx, y));
            y = y.saturating_add(GROUP_HEADER_H);
            last_group = Some(e.group_idx);
        }
        let expanded = state.tile_all_expanded || i == selected;
        let h = if expanded { EXPANDED_H } else { 1 };
        entry_pos.push((y, h));
        y = y.saturating_add(h);
    }
    let total_h = y;

    // Auto-scroll: keep selected entry fully visible.
    let (sel_y, sel_h) = entry_pos[selected];
    let view_h = area.height;
    let mut scroll = state.tile_scroll_row;
    if sel_y < scroll {
        scroll = sel_y;
    }
    if sel_y.saturating_add(sel_h) > scroll.saturating_add(view_h) {
        scroll = sel_y.saturating_add(sel_h).saturating_sub(view_h);
    }
    let max_scroll = total_h.saturating_sub(view_h);
    if scroll > max_scroll {
        scroll = max_scroll;
    }
    state.tile_scroll_row = scroll;

    // Render group headers within viewport.
    for (g_idx, hy) in &header_pos {
        let header_bottom = hy.saturating_add(GROUP_HEADER_H);
        if header_bottom <= scroll || *hy >= scroll.saturating_add(view_h) {
            continue;
        }
        let render_y = area.y.saturating_add(hy.saturating_sub(scroll));
        let header_rect = Rect {
            x: area.x,
            y: render_y,
            width: area.width,
            height: GROUP_HEADER_H,
        };
        let group = &state.repo_groups[*g_idx];
        let count = group.panes.len();
        let attention = group
            .panes
            .iter()
            .filter(|(p, _)| needs_attention(&p.status, p.attention))
            .count();
        draw_group_header(frame, state, header_rect, &group.name, count, attention);
    }

    // Render entries (collapsed / expanded) within viewport. Clone
    // PaneInfo + PaneGitInfo to release the borrow on state before
    // pushing the click target.
    for (i, e) in entries.iter().enumerate() {
        let (py, ph) = entry_pos[i];
        let entry_bottom = py.saturating_add(ph);
        if entry_bottom <= scroll || py >= scroll.saturating_add(view_h) {
            continue;
        }
        let visible_top = py.saturating_sub(scroll);
        let visible_bottom = entry_bottom
            .min(scroll.saturating_add(view_h))
            .saturating_sub(scroll);
        let visible_h = visible_bottom.saturating_sub(visible_top);
        let pane_rect = Rect {
            x: area.x,
            y: area.y.saturating_add(visible_top),
            width: area.width,
            height: visible_h,
        };

        let (pane_clone, info_clone) = {
            let (p, info) = &state.repo_groups[e.group_idx].panes[e.pane_idx];
            (p.clone(), info.clone())
        };
        let pane_id = pane_clone.pane_id.clone();
        let is_selected = i == selected;

        let expanded = state.tile_all_expanded || is_selected;
        if expanded && pane_rect.height >= EXPANDED_H {
            draw_pane_expanded(frame, state, pane_rect, &pane_clone, &info_clone);
        } else {
            // Either not selected, or selected but partly clipped — render
            // just the summary line at the top of the visible area.
            let line_rect = Rect {
                x: pane_rect.x,
                y: pane_rect.y,
                width: pane_rect.width,
                height: 1,
            };
            draw_pane_collapsed(
                frame,
                state,
                line_rect,
                &pane_clone,
                &info_clone,
                is_selected,
            );
        }

        state.layout.tile_targets.push(crate::state::TileTarget {
            rect: pane_rect,
            pane_id,
            row: i,
            col: 0,
        });
    }
}

/// One-line collapsed summary of a pane.
fn draw_pane_collapsed(
    frame: &mut Frame,
    state: &AppState,
    area: Rect,
    pane: &PaneInfo,
    info: &crate::group::PaneGitInfo,
    selected: bool,
) {
    if area.width < 4 || area.height < 1 {
        return;
    }
    let icon = state.icons.status_icon(&pane.status).to_string();
    let status_color = pane_status_color(state, pane);
    let attention =
        pane.attention || matches!(pane.status, PaneStatus::Waiting | PaneStatus::Error);
    let label = pane_label(pane, info);
    let summary = pane_summary_text(pane);
    let status_label = pane_status_label(&pane.status, attention);

    let prefix_str = if selected { "▌ " } else { "  " };
    let prefix_color = if selected {
        state.theme.accent
    } else {
        state.theme.border_inactive
    };

    // Reserve fixed columns for prefix(2), icon(2), glyph(2), label(20).
    let width = area.width as usize;
    const PREFIX_W: usize = 2;
    const ICON_W: usize = 2;
    const GLYPH_W: usize = 2;
    const LABEL_W: usize = 20;
    const STATUS_W: usize = 12;
    let summary_w =
        width.saturating_sub(PREFIX_W + ICON_W + GLYPH_W + LABEL_W + 2 /* gap */ + STATUS_W);

    let label_trim = truncate_to_width(&label, LABEL_W);
    let label_padded = format!("{label_trim:<LABEL_W$}");
    let summary_trim = truncate_to_width(&summary, summary_w);

    let mut spans: Vec<Span> = vec![
        Span::styled(prefix_str.to_string(), Style::default().fg(prefix_color)),
        Span::styled(format!("{icon} "), Style::default().fg(status_color)),
        Span::styled(
            format!("{} ", pane.agent.glyph()),
            Style::default().fg(agent_color(state, &pane.agent)),
        ),
        Span::styled(
            label_padded,
            Style::default()
                .fg(state.theme.branch)
                .add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ),
        Span::raw("  "),
        Span::styled(summary_trim, Style::default().fg(state.theme.text_muted)),
    ];

    // Right-aligned status indicator (running/idle/etc) + attention.
    let used: usize = spans
        .iter()
        .map(|s| super::text::display_width(&s.content))
        .sum();
    let trailing = if attention {
        format!("{status_label} ▲")
    } else {
        status_label.to_string()
    };
    let trailing_w = super::text::display_width(&trailing);
    let pad = width.saturating_sub(used + trailing_w);
    if pad > 0 {
        spans.push(Span::raw(" ".repeat(pad)));
    }
    spans.push(Span::styled(
        trailing,
        Style::default()
            .fg(if attention {
                state.theme.badge_danger
            } else {
                status_color
            })
            .add_modifier(Modifier::BOLD),
    ));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Expanded pane (1 summary line + body lines, indented with `│` bar).
fn draw_pane_expanded(
    frame: &mut Frame,
    state: &AppState,
    area: Rect,
    pane: &PaneInfo,
    info: &crate::group::PaneGitInfo,
) {
    if area.height == 0 {
        return;
    }
    // Top line: same as collapsed but selected.
    let summary_rect = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    draw_pane_collapsed(frame, state, summary_rect, pane, info, true);

    if area.height < 2 {
        return;
    }

    let body_h = area.height - 1;
    let bar_color = state.theme.accent;
    let inner_x = area.x.saturating_add(4); // "    " indent (matches "  " prefix + "│ ")
    let inner_w = area.width.saturating_sub(4);

    let prompt = if !pane.prompt.is_empty() {
        pane.prompt.clone()
    } else if !pane.current_command.is_empty() {
        pane.current_command.clone()
    } else if !pane.wait_reason.is_empty() {
        pane.wait_reason.clone()
    } else {
        "—".into()
    };
    let prompt_trim = truncate_to_width(&prompt, inner_w as usize);

    let permission = pane.permission_mode.badge();
    let mut footer_spans: Vec<Span> = Vec::new();
    if !pane.wait_reason.is_empty() {
        footer_spans.push(Span::styled(
            pane.wait_reason.clone(),
            Style::default().fg(state.theme.wait_reason),
        ));
        footer_spans.push(Span::raw("  ·  "));
    }
    if !permission.is_empty() {
        footer_spans.push(Span::styled(
            permission.to_string(),
            Style::default().fg(state.theme.badge_auto),
        ));
        footer_spans.push(Span::raw("  ·  "));
    }
    let started = pane
        .started_at
        .map(|s| format!("started @ {s}"))
        .unwrap_or_default();
    if !started.is_empty() {
        footer_spans.push(Span::styled(
            started,
            Style::default().fg(state.theme.text_inactive),
        ));
    }

    // Body lines, each prefixed with the `│` bar.
    let bar = Span::styled("  │ ", Style::default().fg(bar_color));

    if body_h >= 1 {
        let line = Line::from(vec![
            bar.clone(),
            Span::styled(prompt_trim, Style::default().fg(state.theme.text_active)),
        ]);
        let row = Rect {
            x: area.x,
            y: area.y.saturating_add(1),
            width: area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(line), row);
    }
    if body_h >= 2 && !footer_spans.is_empty() {
        let mut line_spans = vec![bar];
        line_spans.extend(footer_spans);
        let row = Rect {
            x: area.x,
            y: area.y.saturating_add(2),
            width: area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(Line::from(line_spans)), row);
    }

    // Silence unused warning when inner_x is shadowed by the bar approach.
    let _ = inner_x;
}

fn pane_status_color(state: &AppState, pane: &PaneInfo) -> ratatui::style::Color {
    match pane.status {
        PaneStatus::Running => state.theme.status_running,
        PaneStatus::Background => state.theme.status_running,
        PaneStatus::Waiting => state.theme.status_waiting,
        PaneStatus::Idle => state.theme.status_idle,
        PaneStatus::Error => state.theme.status_error,
        PaneStatus::Unknown => state.theme.status_unknown,
    }
}

fn pane_status_label(status: &PaneStatus, attention: bool) -> &'static str {
    if attention {
        return "attn";
    }
    match status {
        PaneStatus::Running => "running",
        PaneStatus::Background => "bg",
        PaneStatus::Waiting => "waiting",
        PaneStatus::Idle => "idle",
        PaneStatus::Error => "error",
        PaneStatus::Unknown => "—",
    }
}

/// Best label for the row — Claude session name first, then git branch.
fn pane_label(pane: &PaneInfo, info: &crate::group::PaneGitInfo) -> String {
    if !pane.session_name.is_empty() {
        return pane.session_name.clone();
    }
    if let Some(b) = info.branch.as_ref().filter(|s| !s.is_empty()) {
        return b.clone();
    }
    if !pane.worktree.branch.is_empty() {
        return pane.worktree.branch.clone();
    }
    if let Some(w) = info.worktree_name.as_ref().filter(|s| !s.is_empty()) {
        return w.clone();
    }
    "-".into()
}

/// Short summary text for the collapsed row — prompt / command / wait reason.
fn pane_summary_text(pane: &PaneInfo) -> String {
    if !pane.prompt.is_empty() {
        return pane.prompt.clone();
    }
    if !pane.current_command.is_empty() {
        return pane.current_command.clone();
    }
    if !pane.wait_reason.is_empty() {
        return pane.wait_reason.clone();
    }
    String::new()
}
fn draw_group_header(
    frame: &mut Frame,
    state: &AppState,
    area: Rect,
    group_name: &str,
    count: usize,
    attention: usize,
) {
    let mut spans: Vec<Span> = vec![
        Span::styled("── ", Style::default().fg(state.theme.border_inactive)),
        Span::styled(
            group_name.to_string(),
            Style::default()
                .fg(state.theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  ·  {count} agent{}", if count == 1 { "" } else { "s" }),
            Style::default().fg(state.theme.text_muted),
        ),
    ];
    if attention > 0 {
        spans.push(Span::styled(
            format!("  ·  ▲ {attention}"),
            Style::default()
                .fg(state.theme.badge_danger)
                .add_modifier(Modifier::BOLD),
        ));
    }
    let used: usize = spans
        .iter()
        .map(|s| super::text::display_width(&s.content))
        .sum();
    let pad = (area.width as usize).saturating_sub(used + 1);
    if pad > 0 {
        spans.push(Span::styled(
            format!(" {}", "─".repeat(pad)),
            Style::default().fg(state.theme.border_inactive),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
