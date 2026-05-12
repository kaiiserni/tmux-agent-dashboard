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
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
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
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
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

/// Priority key for ordering Summary-tab rows: attention-flagged first,
/// then by status urgency (Waiting > Error > Idle > Running >
/// Background > Unknown), then most-recent start time first. The Tiles
/// tab does NOT use this — its group order stays alphabetical so `d`/`u`
/// navigation is stable.
fn pane_priority_key(p: &PaneInfo) -> (u8, u8, std::cmp::Reverse<u64>) {
    let attention = if p.attention { 0 } else { 1 };
    let status = match p.status {
        PaneStatus::Waiting => 0,
        PaneStatus::Error => 1,
        PaneStatus::Idle => 2,
        PaneStatus::Running => 3,
        PaneStatus::Background => 3,
        PaneStatus::Unknown => 4,
    };
    (
        attention,
        status,
        std::cmp::Reverse(p.started_at.unwrap_or(0)),
    )
}

/// Collect `(group_name, pane, info)` triples across every group, sorted
/// by pane priority (urgent first). Use for the Summary tab so the Idle
/// / Waiting / etc. lists show the most-urgent panes first regardless of
/// which repo they belong to.
fn sorted_summary_panes(
    state: &AppState,
) -> Vec<(String, &PaneInfo, &crate::group::PaneGitInfo)> {
    let mut v: Vec<_> = state
        .repo_groups
        .iter()
        .flat_map(|g| {
            g.panes
                .iter()
                .map(move |(p, info)| (g.name.clone(), p, info))
        })
        .collect();
    v.sort_by(|a, b| pane_priority_key(a.1).cmp(&pane_priority_key(b.1)));
    v
}

fn draw_attention(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let block = block_with_title(state, "needs attention");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Collect attention-flagged panes (notification / permission_denied / teammate_idle).
    let rows: Vec<SummaryRow> = sorted_summary_panes(state)
        .into_iter()
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

    let rows: Vec<SummaryRow> = sorted_summary_panes(state)
        .into_iter()
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

    let rows: Vec<SummaryRow> = sorted_summary_panes(state)
        .into_iter()
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
    let mut rows: Vec<(SummaryRow, std::time::SystemTime)> = sorted_summary_panes(state)
        .into_iter()
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
    let mut rows: Vec<(SummaryRow, u64)> = sorted_summary_panes(state)
        .into_iter()
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
    let mut rows: Vec<(SummaryRow, std::time::SystemTime)> = sorted_summary_panes(state)
        .into_iter()
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

// ── tiles view ──────────────────────────────────────────────────────

/// Best-effort stable label for a tile. Prefers the Claude session name
/// (`/rename` label) since that's the user's chosen identity for the
/// agent, then falls back through git branch, worktree, working
/// directory basename, and pane id so the label never flickers to a
/// placeholder while async data is in flight.
fn tile_label(pane: &PaneInfo, info: &crate::group::PaneGitInfo) -> String {
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
    if !pane.path.is_empty()
        && let Some(base) = std::path::Path::new(&pane.path)
            .file_name()
            .and_then(|s| s.to_str())
        && !base.is_empty()
    {
        return base.to_string();
    }
    pane.pane_id.clone()
}

const TILE_TARGET_W: u16 = 38;
const TILE_H: u16 = 6;
const TILE_GAP_V: u16 = 1;
const GROUP_HEADER_H: u16 = 1;
const GROUP_SPACER_H: u16 = 1;

fn draw_tiles(frame: &mut Frame, state: &mut AppState, area: Rect) {
    state.layout.tile_targets.clear();
    state.layout.tile_cols = 0;
    state.layout.tile_visible_first = 0;
    state.layout.tile_visible_last = 0;

    // Keep the "one group open" invariant. If `expanded_group` is None
    // or points at a group that no longer exists / is empty, fall back
    // to the first non-empty group.
    crate::app::input::ensure_expanded_group(state);

    if state.repo_groups.is_empty() || state.repo_groups.iter().all(|g| g.panes.is_empty()) {
        let para = Paragraph::new(Line::from(Span::styled(
            "  no agents to show",
            Style::default().fg(state.theme.text_muted),
        )));
        frame.render_widget(para, area);
        return;
    }

    let cols = (area.width / TILE_TARGET_W).max(1) as usize;
    state.layout.tile_cols = cols;

    // Clamp scroll to a valid group index.
    let last_group = state.repo_groups.len().saturating_sub(1);
    if state.tile_scroll_group > last_group {
        state.tile_scroll_group = last_group;
    }

    let mut constraints: Vec<Constraint> = Vec::new();
    let mut group_indices: Vec<usize> = Vec::new();
    let mut used: u16 = 0;
    for (idx, group) in state
        .repo_groups
        .iter()
        .enumerate()
        .skip(state.tile_scroll_group)
    {
        if group.panes.is_empty() {
            continue;
        }
        let folded = state.expanded_group.as_deref() != Some(group.key.as_str());
        let group_h = if folded {
            GROUP_HEADER_H + GROUP_SPACER_H
        } else {
            let tile_rows = group.panes.len().div_ceil(cols) as u16;
            GROUP_HEADER_H
                + tile_rows * TILE_H
                + tile_rows.saturating_sub(1) * TILE_GAP_V
                + GROUP_SPACER_H
        };
        if used + group_h > area.height && !group_indices.is_empty() {
            break;
        }
        constraints.push(Constraint::Length(group_h));
        group_indices.push(idx);
        used = used.saturating_add(group_h);
    }
    constraints.push(Constraint::Min(0));

    if let (Some(first), Some(last)) = (group_indices.first(), group_indices.last()) {
        state.layout.tile_visible_first = *first;
        state.layout.tile_visible_last = *last;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let col_constraints: Vec<Constraint> = (0..cols)
        .map(|_| Constraint::Ratio(1, cols as u32))
        .collect();

    // One-group-expanded model: `tile_selected` is a local index into the
    // expanded group's panes (0..len). Clamp if the expanded group shrank.
    if let Some(expanded_key) = state.expanded_group.clone()
        && let Some(group) = state.repo_groups.iter().find(|g| g.key == expanded_key)
        && !group.panes.is_empty()
        && state.tile_selected >= group.panes.len()
    {
        state.tile_selected = group.panes.len() - 1;
    }

    let mut global_row: usize = 0;
    for (slot_idx, group_idx) in group_indices.iter().enumerate() {
        let group_idx = *group_idx;
        let section = sections[slot_idx];
        let group_name = state.repo_groups[group_idx].name.clone();
        let group_key = state.repo_groups[group_idx].key.clone();
        let folded = state.expanded_group.as_deref() != Some(group_key.as_str());

        if folded {
            // Header only — skip the grid body.
            let parts = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(GROUP_HEADER_H),
                    Constraint::Length(GROUP_SPACER_H),
                ])
                .split(section);
            let group_panes_count = state.repo_groups[group_idx].panes.len();
            let group_attention = state.repo_groups[group_idx]
                .panes
                .iter()
                .filter(|(p, _)| needs_attention(&p.status, p.attention))
                .count();
            draw_group_header(
                frame,
                state,
                parts[0],
                &group_name,
                group_panes_count,
                group_attention,
            );
            continue;
        }

        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(GROUP_HEADER_H),
                Constraint::Min(0),
                Constraint::Length(GROUP_SPACER_H),
            ])
            .split(section);

        let group_panes_count = state.repo_groups[group_idx].panes.len();
        let group_attention = state.repo_groups[group_idx]
            .panes
            .iter()
            .filter(|(p, _)| needs_attention(&p.status, p.attention))
            .count();
        draw_group_header(
            frame,
            state,
            parts[0],
            &group_name,
            group_panes_count,
            group_attention,
        );

        let group_rows = group_panes_count.div_ceil(cols);
        draw_group_tiles(
            frame,
            state,
            parts[1],
            group_idx,
            &col_constraints,
            global_row,
        );
        global_row += group_rows;
    }
}

fn draw_group_tiles(
    frame: &mut Frame,
    state: &mut AppState,
    area: Rect,
    group_idx: usize,
    col_constraints: &[Constraint],
    row_offset: usize,
) {
    let cols = col_constraints.len();
    let panes_len = state.repo_groups[group_idx].panes.len();
    let rows = panes_len.div_ceil(cols);
    let mut row_constraints: Vec<Constraint> = Vec::new();
    for i in 0..rows {
        row_constraints.push(Constraint::Length(TILE_H));
        if i + 1 < rows {
            row_constraints.push(Constraint::Length(TILE_GAP_V));
        }
    }
    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    for idx in 0..panes_len {
        let row = idx / cols;
        let col = idx % cols;
        let row_area_idx = row * 2;
        if row_area_idx >= row_areas.len() {
            break;
        }
        let cell_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints.to_vec())
            .split(row_areas[row_area_idx]);
        let cell = cell_areas[col];
        let gutter = if col + 1 < cols { 1 } else { 0 };
        let padded = Rect {
            x: cell.x,
            y: cell.y,
            width: cell.width.saturating_sub(gutter),
            height: cell.height,
        };

        let tile_idx = state.layout.tile_targets.len();
        let selected = tile_idx == state.tile_selected;

        let (pane_clone, info_clone) = {
            let (p, info) = &state.repo_groups[group_idx].panes[idx];
            (p.clone(), info.clone())
        };
        let pane_id = pane_clone.pane_id.clone();
        draw_tile(frame, state, padded, &pane_clone, &info_clone, selected);
        state.layout.tile_targets.push(crate::state::TileTarget {
            rect: padded,
            pane_id,
            row: row_offset + row,
            col,
        });
    }
}

fn draw_tile(
    frame: &mut Frame,
    state: &AppState,
    area: Rect,
    pane: &PaneInfo,
    info: &crate::group::PaneGitInfo,
    selected: bool,
) {
    if area.width < 10 || area.height < 4 {
        return;
    }
    let (color, status_label) = match pane.status {
        PaneStatus::Running => (state.theme.status_running, "running"),
        PaneStatus::Background => (state.theme.status_running, "background"),
        PaneStatus::Waiting => (state.theme.status_waiting, "waiting"),
        PaneStatus::Idle => (state.theme.status_idle, "idle"),
        PaneStatus::Error => (state.theme.status_error, "error"),
        PaneStatus::Unknown => (state.theme.status_unknown, "—"),
    };

    let attention =
        pane.attention || matches!(pane.status, PaneStatus::Waiting | PaneStatus::Error);
    let border_color = if selected {
        state.theme.accent
    } else if attention {
        state.theme.badge_danger
    } else {
        state.theme.border_inactive
    };

    let icon = state.icons.status_icon(&pane.status);
    let branch = tile_label(pane, info);

    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);
    let accent_area = split[0];
    let body_area = split[1];

    let accent_lines: Vec<Line> = (0..accent_area.height)
        .map(|_| Line::from(Span::styled("▎", Style::default().fg(color))))
        .collect();
    frame.render_widget(Paragraph::new(accent_lines), accent_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(if selected {
            BorderType::Thick
        } else {
            BorderType::Rounded
        })
        .padding(Padding::horizontal(1))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(icon.to_string(), Style::default().fg(color)),
            Span::raw(" "),
            Span::styled(
                pane.agent.glyph(),
                Style::default().fg(agent_color(state, &pane.agent)),
            ),
            Span::raw(" "),
            Span::styled(
                format!("{branch} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]))
        .style(Style::default().fg(border_color));
    let inner = block.inner(body_area);
    frame.render_widget(block, body_area);

    let width = inner.width as usize;

    let prompt = if !pane.prompt.is_empty() {
        pane.prompt.clone()
    } else if !pane.current_command.is_empty() {
        pane.current_command.clone()
    } else if !pane.wait_reason.is_empty() {
        pane.wait_reason.clone()
    } else {
        "—".into()
    };

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        truncate_to_width(&prompt, width),
        Style::default().fg(state.theme.text_active),
    )));

    let badge = pane.permission_mode.badge();
    let mut footer: Vec<Span> = vec![Span::styled(
        status_label.to_string(),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )];
    if !badge.is_empty() {
        footer.push(Span::raw("  "));
        footer.push(Span::styled(
            badge.to_string(),
            Style::default().fg(state.theme.badge_auto),
        ));
    }
    if attention {
        let used: usize = footer
            .iter()
            .map(|s| super::text::display_width(&s.content))
            .sum();
        let attention_text = if pane.wait_reason.is_empty() {
            "▲".to_string()
        } else {
            format!(
                "▲ {}",
                truncate_to_width(&pane.wait_reason, width.saturating_sub(used + 4))
            )
        };
        let attn_w = super::text::display_width(&attention_text);
        let pad = width.saturating_sub(used + attn_w);
        if pad > 0 {
            footer.push(Span::raw(" ".repeat(pad)));
        }
        footer.push(Span::styled(
            attention_text,
            Style::default()
                .fg(state.theme.badge_danger)
                .add_modifier(Modifier::BOLD),
        ));
    }

    while lines.len() + 1 < inner.height as usize {
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(footer));

    frame.render_widget(Paragraph::new(lines), inner);
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
