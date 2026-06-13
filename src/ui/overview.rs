//! Overview tab — renders the agent-overview job's snapshot
//! (`overview.json`) as a scrollable, attention-first briefing: TL;DR,
//! one block per project (panes, what's happening, what needs the
//! developer, next steps, matching _ACTIVE.md notes), idle panes last.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::overview::Overview;
use crate::state::{AppState, OverviewTarget};

use super::text::display_width;

/// One rendered row plus the pane it jumps to when clicked (if any).
/// `link` is (pane_id, "session:window.pane") — the target string backs
/// up the pane id when the snapshot has gone stale.
struct Row {
    line: Line<'static>,
    link: Option<(String, String)>,
}

fn redact(state: &AppState, s: &str) -> String {
    if state.privacy_mode {
        super::text::obfuscate(s)
    } else {
        s.to_string()
    }
}

/// Greedy word wrap on display width. Returns at least one (possibly
/// empty) line so callers can map text 1:1 to rows.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let cur_w = display_width(&cur);
        let word_w = display_width(word);
        if cur.is_empty() {
            cur = word.to_string();
        } else if cur_w + 1 + word_w <= width {
            cur.push(' ');
            cur.push_str(word);
        } else {
            out.push(cur);
            cur = word.to_string();
        }
    }
    out.push(cur);
    out
}

fn status_color(state: &AppState, status: &str) -> ratatui::style::Color {
    match status {
        "running" | "background" => state.theme.status_running,
        "waiting" => state.theme.status_waiting,
        "error" => state.theme.status_error,
        "idle" => state.theme.status_idle,
        _ => state.theme.status_unknown,
    }
}

pub fn draw_overview(frame: &mut Frame, state: &mut AppState, area: Rect) {
    state.layout.overview_targets = Vec::new();
    state.layout.overview_anchors = Vec::new();
    state.layout.overview_view_height = area.height as usize;

    let Some(overview) = state.overview.clone() else {
        let msg = Paragraph::new(Line::from(Span::styled(
            "No overview yet — the agent-overview job hasn't produced output.",
            Style::default().fg(state.theme.text_inactive),
        )));
        frame.render_widget(msg, area);
        state.layout.overview_total_lines = 1;
        return;
    };

    let width = area.width as usize;
    let rows = build_rows(state, &overview, width);

    // All navigable rows (full list, absolute index) — for keyboard nav.
    for (idx, row) in rows.iter().enumerate() {
        if let Some((pane_id, target)) = &row.link {
            state.layout.overview_anchors.push(crate::state::OverviewAnchor {
                row: idx,
                pane_id: pane_id.clone(),
                target: target.clone(),
            });
        }
    }
    let anchors = &state.layout.overview_anchors;
    if !anchors.is_empty() && state.overview_selected >= anchors.len() {
        state.overview_selected = anchors.len() - 1;
    }
    let selected_row = anchors.get(state.overview_selected).map(|a| a.row);

    state.layout.overview_total_lines = rows.len();
    let height = area.height as usize;
    let max_scroll = rows.len().saturating_sub(height);
    // Keep the keyboard selection in view.
    if let Some(sel) = selected_row {
        if sel < state.overview_scroll {
            state.overview_scroll = sel;
        } else if sel >= state.overview_scroll + height {
            state.overview_scroll = sel + 1 - height;
        }
    }
    if state.overview_scroll > max_scroll {
        state.overview_scroll = max_scroll;
    }
    let scroll = state.overview_scroll;

    let sel_style = Style::default().bg(state.theme.selection_bg);
    let visible: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(scroll)
        .take(height)
        .map(|(idx, r)| {
            let line = r.line.clone();
            if Some(idx) == selected_row {
                line.patch_style(sel_style)
            } else {
                line
            }
        })
        .collect();

    // Register click targets for the visible pane rows (mouse).
    for (offset, row) in rows.iter().skip(scroll).take(height).enumerate() {
        if let Some((pane_id, target)) = &row.link {
            state.layout.overview_targets.push(OverviewTarget {
                rect: Rect {
                    x: area.x,
                    y: area.y + offset as u16,
                    width: area.width,
                    height: 1,
                },
                pane_id: pane_id.clone(),
                target: target.clone(),
            });
        }
    }

    frame.render_widget(Paragraph::new(visible), area);
}

fn build_rows(state: &AppState, overview: &Overview, width: usize) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    let muted = Style::default().fg(state.theme.text_muted);
    let inactive = Style::default().fg(state.theme.text_inactive);
    let title_style = Style::default()
        .fg(state.theme.section_title)
        .add_modifier(Modifier::BOLD);

    fn push(rows: &mut Vec<Row>, line: Line<'static>) {
        rows.push(Row { line, link: None });
    }

    /// Push a row that jumps to `link` when clicked (project block lines
    /// link through to the project's most relevant pane).
    fn push_link(rows: &mut Vec<Row>, line: Line<'static>, link: &Option<(String, String)>) {
        rows.push(Row { line, link: link.clone() });
    }

    // ── Status line ──────────────────────────────────────────────────
    let ago = crate::time::compact_ago(overview.updated_at).unwrap_or_else(|| "?".into());
    let active_panes: usize = overview.projects.iter().map(|p| p.panes.len()).sum();
    push(&mut rows, Line::from(Span::styled(
        format!(
            "Updated {ago} ago · {} project{} · {active_panes} active pane{} · {} idle",
            overview.projects.len(),
            if overview.projects.len() == 1 { "" } else { "s" },
            if active_panes == 1 { "" } else { "s" },
            overview.idle.len(),
        ),
        inactive,
    )));
    push(&mut rows, Line::default());

    // ── TL;DR ────────────────────────────────────────────────────────
    if !overview.tldr.is_empty() {
        push(&mut rows, Line::from(Span::styled("TL;DR", title_style)));
        for item in &overview.tldr {
            let wrapped = wrap(&redact(state, item), width.saturating_sub(4));
            for (i, part) in wrapped.into_iter().enumerate() {
                let prefix = if i == 0 { "  • " } else { "    " };
                push(&mut rows, Line::from(vec![
                    Span::styled(prefix.to_string(), Style::default().fg(state.theme.accent)),
                    Span::styled(part, Style::default().fg(state.theme.text_active)),
                ]));
            }
        }
        push(&mut rows, Line::default());
    }

    // ── Projects ─────────────────────────────────────────────────────
    for project in &overview.projects {
        // Whole-block click target: prefer a waiting/error pane, else the
        // first one.
        let link: Option<(String, String)> = project
            .panes
            .iter()
            .find(|p| matches!(p.status.as_str(), "waiting" | "error"))
            .or_else(|| project.panes.first())
            .map(|p| (p.pane_id.clone(), p.target.clone()));
        let head_color = if project.attention {
            state.theme.status_waiting
        } else {
            state.theme.section_title
        };
        let mut head = vec![Span::styled(
            format!("▍{}", redact(state, &project.name)),
            Style::default().fg(head_color).add_modifier(Modifier::BOLD),
        )];
        if project.attention {
            head.push(Span::styled(
                "  ⚠ needs you",
                Style::default().fg(state.theme.status_waiting),
            ));
        }
        push_link(&mut rows, Line::from(head), &link);

        for pane in &project.panes {
            let age = pane
                .age_minutes
                .filter(|_| !state.privacy_mode)
                .map(|m| {
                    if m >= 60 {
                        format!(" · {}h{:02}", m / 60, m % 60)
                    } else {
                        format!(" · {m}m")
                    }
                })
                .unwrap_or_default();
            let line = Line::from(vec![
                Span::styled("  ▸ ".to_string(), Style::default().fg(state.theme.accent)),
                Span::styled(pane.target.clone(), Style::default().fg(state.theme.text_active)),
                Span::styled(format!(" · {}", pane.agent), muted),
                Span::styled(
                    format!(" · {}", pane.status),
                    Style::default().fg(status_color(state, &pane.status)),
                ),
                Span::styled(age, muted),
            ]);
            rows.push(Row {
                line,
                link: Some((pane.pane_id.clone(), pane.target.clone())),
            });
            if !pane.summary.is_empty() {
                for part in wrap(&redact(state, &pane.summary), width.saturating_sub(6)) {
                    rows.push(Row {
                        line: Line::from(Span::styled(format!("      {part}"), muted)),
                        link: Some((pane.pane_id.clone(), pane.target.clone())),
                    });
                }
            }
        }

        if !project.doing.is_empty() {
            for part in wrap(&redact(state, &project.doing), width.saturating_sub(2)) {
                push_link(&mut rows, Line::from(Span::styled(format!("  {part}"), muted)), &link);
            }
        }
        if !project.needs_from_you.is_empty() && project.needs_from_you != "null" {
            let wrapped = wrap(
                &redact(state, &project.needs_from_you),
                width.saturating_sub(13),
            );
            for (i, part) in wrapped.into_iter().enumerate() {
                let prefix = if i == 0 { "  ⚠ Needs you: " } else { "    " };
                push_link(&mut rows, Line::from(vec![
                    Span::styled(
                        prefix.to_string(),
                        Style::default()
                            .fg(state.theme.status_waiting)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(part, Style::default().fg(state.theme.text_active)),
                ]), &link);
            }
        }
        for step in &project.next_steps {
            let wrapped = wrap(&redact(state, step), width.saturating_sub(6));
            for (i, part) in wrapped.into_iter().enumerate() {
                let prefix = if i == 0 { "  → " } else { "    " };
                push_link(&mut rows, Line::from(vec![
                    Span::styled(prefix.to_string(), Style::default().fg(state.theme.response_arrow)),
                    Span::styled(part, Style::default().fg(state.theme.text_active)),
                ]), &link);
            }
        }
        for note in &project.active_md {
            for part in wrap(&redact(state, note), width.saturating_sub(4)) {
                push_link(&mut rows, Line::from(Span::styled(
                    format!("    {part}"),
                    Style::default()
                        .fg(state.theme.text_inactive)
                        .add_modifier(Modifier::ITALIC),
                )), &link);
            }
        }
        push(&mut rows, Line::default());
    }

    // ── Idle ─────────────────────────────────────────────────────────
    if !overview.idle.is_empty() {
        rows.push(Row {
            line: Line::from(Span::styled(
                format!("Idle ({})", overview.idle.len()),
                title_style,
            )),
            link: None,
        });
        for pane in &overview.idle {
            let task = redact(state, &pane.task);
            let line = Line::from(vec![
                Span::styled(format!("  {} · ", pane.target), inactive),
                Span::styled(redact(state, &pane.project), inactive),
                Span::styled(
                    if task.is_empty() {
                        String::new()
                    } else {
                        format!(" — {task}")
                    },
                    inactive,
                ),
            ]);
            rows.push(Row {
                line,
                link: Some((pane.pane_id.clone(), pane.target.clone())),
            });
        }
    }

    rows
}
