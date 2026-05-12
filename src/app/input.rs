use std::io;

use crossterm::event::{Event, KeyCode, MouseButton, MouseEventKind};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::state::{AppState, DashboardTab, SummarySection};

pub(super) fn handle_event(
    ev: Event,
    state: &mut AppState,
    _terminal: &Terminal<CrosstermBackend<io::Stdout>>,
) -> bool {
    let needs_redraw = false;

    if let Event::Key(key) = &ev {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                state.should_exit = true;
                return true;
            }
            KeyCode::Tab => {
                state.dashboard_tab = match state.dashboard_tab {
                    DashboardTab::Summary => DashboardTab::Tiles,
                    DashboardTab::Tiles => DashboardTab::Summary,
                };
                return true;
            }
            KeyCode::Char('s') => {
                state.sort_by_activity = !state.sort_by_activity;
                state.sort_groups_if_needed();
                return true;
            }
            _ => {}
        }

        if state.dashboard_tab == DashboardTab::Tiles && handle_dashboard_tiles_key(state, key.code)
        {
            return true;
        }
        if state.dashboard_tab == DashboardTab::Summary
            && handle_dashboard_summary_key(state, key.code)
        {
            return true;
        }
    }

    if let Event::Mouse(mouse) = &ev {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => match state.dashboard_tab {
                DashboardTab::Tiles => {
                    if let Some(idx) = find_tile_at(state, mouse.row, mouse.column) {
                        state.tile_selected = idx;
                        if let Some(target) = state.layout.tile_targets.get(idx).cloned() {
                            state.activate_pane_by_id(&target.pane_id);
                            state.should_exit = true;
                        }
                        return true;
                    }
                }
                DashboardTab::Summary => {
                    if let Some(idx) = find_summary_at(state, mouse.row, mouse.column) {
                        state.summary_selected = idx;
                        if let Some(target) = state.layout.summary_targets.get(idx).cloned() {
                            state.activate_pane_by_id(&target.pane_id);
                            state.should_exit = true;
                        }
                        return true;
                    }
                }
            },
            MouseEventKind::ScrollDown if state.dashboard_tab == DashboardTab::Tiles => {
                let last_group = state.repo_groups.len().saturating_sub(1);
                if state.tile_scroll_group < last_group {
                    state.tile_scroll_group += 1;
                }
                return true;
            }
            MouseEventKind::ScrollUp if state.dashboard_tab == DashboardTab::Tiles => {
                if state.tile_scroll_group > 0 {
                    state.tile_scroll_group -= 1;
                }
                return true;
            }
            MouseEventKind::ScrollDown if state.dashboard_tab == DashboardTab::Summary => {
                if let Some(section) = summary_section_at(state, mouse.row, mouse.column) {
                    scroll_summary_section(state, section, 1);
                    return true;
                }
            }
            MouseEventKind::ScrollUp if state.dashboard_tab == DashboardTab::Summary => {
                if let Some(section) = summary_section_at(state, mouse.row, mouse.column) {
                    scroll_summary_section(state, section, -1);
                    return true;
                }
            }
            _ => {}
        }
    }

    needs_redraw
}

// ─── Tiles tab navigation ───────────────────────────────────────────

fn handle_dashboard_tiles_key(state: &mut AppState, code: KeyCode) -> bool {
    let total = state.layout.tile_targets.len();
    if total == 0 {
        return match code {
            KeyCode::Char('f') => {
                toggle_fold_all(state);
                true
            }
            _ => false,
        };
    }
    let cur = state.tile_selected.min(total - 1);
    let (cur_row, cur_col) = {
        let t = &state.layout.tile_targets[cur];
        (t.row, t.col)
    };

    match code {
        KeyCode::Char('h') | KeyCode::Left => {
            if let Some(idx) = state
                .layout
                .tile_targets
                .iter()
                .enumerate()
                .filter(|(_, t)| t.row == cur_row && t.col < cur_col)
                .max_by_key(|(_, t)| t.col)
                .map(|(i, _)| i)
                .or(if cur > 0 { Some(cur - 1) } else { None })
            {
                state.tile_selected = idx;
            }
            true
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(idx) = state
                .layout
                .tile_targets
                .iter()
                .enumerate()
                .filter(|(_, t)| t.row == cur_row && t.col > cur_col)
                .min_by_key(|(_, t)| t.col)
                .map(|(i, _)| i)
                .or(if cur + 1 < total { Some(cur + 1) } else { None })
            {
                state.tile_selected = idx;
            }
            true
        }
        KeyCode::Char('j') | KeyCode::Down => {
            if let Some(idx) = nearest_in_direction(state, cur_row, cur_col, true) {
                state.tile_selected = idx;
            } else {
                let last_group = state.repo_groups.len().saturating_sub(1);
                if state.layout.tile_visible_last < last_group {
                    state.tile_scroll_group += 1;
                    state.tile_selected = 0;
                }
            }
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if let Some(idx) = nearest_in_direction(state, cur_row, cur_col, false) {
                state.tile_selected = idx;
            } else if state.tile_scroll_group > 0 {
                state.tile_scroll_group -= 1;
                state.tile_selected = usize::MAX;
            }
            true
        }
        KeyCode::PageDown => {
            let last_group = state.repo_groups.len().saturating_sub(1);
            if state.tile_scroll_group < last_group {
                state.tile_scroll_group += 1;
                state.tile_selected = 0;
            }
            true
        }
        KeyCode::PageUp => {
            if state.tile_scroll_group > 0 {
                state.tile_scroll_group -= 1;
                state.tile_selected = 0;
            }
            true
        }
        KeyCode::Char('m') => {
            if let Some(target) = state.layout.tile_targets.get(cur).cloned() {
                toggle_tile_mark(state, &target.pane_id);
            }
            true
        }
        KeyCode::Char('f') => {
            toggle_fold_all(state);
            true
        }
        KeyCode::Char('z') => {
            toggle_fold_current(state, cur);
            true
        }
        KeyCode::Enter => {
            if let Some(target) = state.layout.tile_targets.get(cur).cloned() {
                state.activate_pane_by_id(&target.pane_id);
                state.should_exit = true;
            }
            true
        }
        _ => false,
    }
}

fn nearest_in_direction(
    state: &AppState,
    cur_row: usize,
    cur_col: usize,
    down: bool,
) -> Option<usize> {
    let candidates: Vec<(usize, &crate::state::TileTarget)> = state
        .layout
        .tile_targets
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            if down {
                t.row > cur_row
            } else {
                t.row < cur_row
            }
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let target_row = if down {
        candidates.iter().map(|(_, t)| t.row).min().unwrap()
    } else {
        candidates.iter().map(|(_, t)| t.row).max().unwrap()
    };
    candidates
        .into_iter()
        .filter(|(_, t)| t.row == target_row)
        .min_by_key(|(_, t)| (t.col as isize - cur_col as isize).abs())
        .map(|(i, _)| i)
}

/// Toggle fold-all: if ANY group is currently unfolded, fold everything;
/// else unfold everything.
fn toggle_fold_all(state: &mut AppState) {
    let any_unfolded = state
        .repo_groups
        .iter()
        .any(|g| !state.folded_groups.contains(&g.name));
    if any_unfolded {
        state.folded_groups = state.repo_groups.iter().map(|g| g.name.clone()).collect();
    } else {
        state.folded_groups.clear();
    }
}

/// Toggle fold state of the group containing the currently selected
/// tile (resolved via `tile_targets[cur]`).
fn toggle_fold_current(state: &mut AppState, cur: usize) {
    let target = match state.layout.tile_targets.get(cur).cloned() {
        Some(t) => t,
        None => return,
    };
    let group_name = state.repo_groups.iter().find_map(|g| {
        g.panes
            .iter()
            .find(|(p, _)| p.pane_id == target.pane_id)
            .map(|_| g.name.clone())
    });
    if let Some(name) = group_name
        && !state.folded_groups.remove(&name)
    {
        state.folded_groups.insert(name);
    }
}

fn toggle_tile_mark(state: &mut AppState, pane_id: &str) {
    use crate::tmux;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut set_value: Option<u64> = None;
    for group in state.repo_groups.iter_mut() {
        for (pane, _) in group.panes.iter_mut() {
            if pane.pane_id == pane_id {
                if pane.marked_unread_at.is_some() {
                    pane.marked_unread_at = None;
                    tmux::unset_pane_option(pane_id, tmux::PANE_MARKED_UNREAD_AT);
                } else {
                    pane.marked_unread_at = Some(now);
                    set_value = Some(now);
                }
                break;
            }
        }
    }
    if let Some(v) = set_value {
        tmux::set_pane_option(pane_id, tmux::PANE_MARKED_UNREAD_AT, &v.to_string());
    }
}

fn find_tile_at(state: &AppState, row: u16, col: u16) -> Option<usize> {
    state.layout.tile_targets.iter().position(|t| {
        t.rect
            .contains(ratatui::layout::Position { x: col, y: row })
    })
}

// ─── Summary tab navigation ─────────────────────────────────────────

fn handle_dashboard_summary_key(state: &mut AppState, code: KeyCode) -> bool {
    let total = state.layout.summary_targets.len();
    if total == 0 {
        return match code {
            KeyCode::PageDown => {
                if let Some(section) = first_nonempty_section(state) {
                    scroll_summary_section(state, section, 1);
                    return true;
                }
                false
            }
            KeyCode::PageUp => {
                if let Some(section) = first_nonempty_section(state) {
                    scroll_summary_section(state, section, -1);
                    return true;
                }
                false
            }
            _ => false,
        };
    }
    let cur = state.summary_selected.min(total - 1);
    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            let cur_section = state.layout.summary_targets[cur].section;
            if cur + 1 < total {
                let next_section = state.layout.summary_targets[cur + 1].section;
                if next_section == cur_section || !section_has_hidden_below(state, cur_section) {
                    state.summary_selected = cur + 1;
                } else {
                    scroll_summary_section(state, cur_section, 1);
                }
            } else if section_has_hidden_below(state, cur_section) {
                scroll_summary_section(state, cur_section, 1);
            }
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let cur_section = state.layout.summary_targets[cur].section;
            if cur > 0 {
                let prev_section = state.layout.summary_targets[cur - 1].section;
                if prev_section == cur_section || !section_has_hidden_above(state, cur_section) {
                    state.summary_selected = cur - 1;
                } else {
                    scroll_summary_section(state, cur_section, -1);
                }
            } else if section_has_hidden_above(state, cur_section) {
                scroll_summary_section(state, cur_section, -1);
            }
            true
        }
        KeyCode::Char('h') | KeyCode::Left => {
            if let Some(idx) = nearest_in_other_column(state, cur, false) {
                state.summary_selected = idx;
            }
            true
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if let Some(idx) = nearest_in_other_column(state, cur, true) {
                state.summary_selected = idx;
            }
            true
        }
        KeyCode::Char('g') => {
            state.summary_selected = 0;
            for s in [
                SummarySection::Attention,
                SummarySection::Waiting,
                SummarySection::Responded,
                SummarySection::Running,
                SummarySection::MarkedUnread,
                SummarySection::Idle,
            ] {
                set_section_scroll(state, s, 0);
            }
            true
        }
        KeyCode::Char('G') => {
            state.summary_selected = total - 1;
            true
        }
        KeyCode::PageDown => {
            let cur_section = state.layout.summary_targets[cur].section;
            scroll_summary_section(state, cur_section, 3);
            true
        }
        KeyCode::PageUp => {
            let cur_section = state.layout.summary_targets[cur].section;
            scroll_summary_section(state, cur_section, -3);
            true
        }
        KeyCode::Char('m') => {
            if let Some(target) = state.layout.summary_targets.get(cur).cloned() {
                toggle_mark(state, &target.pane_id);
            }
            true
        }
        KeyCode::Enter => {
            if let Some(target) = state.layout.summary_targets.get(cur).cloned() {
                state.activate_pane_by_id(&target.pane_id);
                state.should_exit = true;
            }
            true
        }
        _ => false,
    }
}

/// Toggle the `@dashboard_marked_unread_at` flag on a pane. Sets the
/// current epoch if absent, unsets if present. Mutates AppState's
/// in-memory pane so the next render reflects the new state without
/// waiting for a refresh.
fn toggle_mark(state: &mut AppState, pane_id: &str) {
    use crate::tmux;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut new_value: Option<u64> = None;
    for group in state.repo_groups.iter_mut() {
        for (pane, _) in group.panes.iter_mut() {
            if pane.pane_id == pane_id {
                if pane.marked_unread_at.is_some() {
                    pane.marked_unread_at = None;
                    tmux::unset_pane_option(pane_id, tmux::PANE_MARKED_UNREAD_AT);
                } else {
                    pane.marked_unread_at = Some(now);
                    new_value = Some(now);
                }
                break;
            }
        }
    }
    if let Some(v) = new_value {
        tmux::set_pane_option(pane_id, tmux::PANE_MARKED_UNREAD_AT, &v.to_string());
    }
}

fn is_left_column(section: SummarySection) -> bool {
    matches!(
        section,
        SummarySection::Attention | SummarySection::Waiting | SummarySection::Responded
    )
}

fn nearest_in_other_column(state: &AppState, cur: usize, target_right: bool) -> Option<usize> {
    let cur_target = state.layout.summary_targets.get(cur)?;
    let cur_y = cur_target.rect.y as i32;
    state
        .layout
        .summary_targets
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            let in_right = !is_left_column(t.section);
            in_right == target_right
        })
        .min_by_key(|(_, t)| (t.rect.y as i32 - cur_y).abs())
        .map(|(i, _)| i)
}

fn find_summary_at(state: &AppState, row: u16, col: u16) -> Option<usize> {
    state.layout.summary_targets.iter().position(|t| {
        t.rect
            .contains(ratatui::layout::Position { x: col, y: row })
    })
}

// ─── Summary section scroll helpers ─────────────────────────────────

pub fn section_rect_for(
    state: &AppState,
    section: SummarySection,
) -> &crate::state::SummarySectionRect {
    match section {
        SummarySection::Attention => &state.layout.summary_section_attention,
        SummarySection::Waiting => &state.layout.summary_section_waiting,
        SummarySection::Responded => &state.layout.summary_section_responded,
        SummarySection::Running => &state.layout.summary_section_running,
        SummarySection::MarkedUnread => &state.layout.summary_section_marked_unread,
        SummarySection::Idle => &state.layout.summary_section_idle,
    }
}

pub fn section_scroll_get(state: &AppState, section: SummarySection) -> usize {
    match section {
        SummarySection::Attention => state.summary_scroll_attention,
        SummarySection::Waiting => state.summary_scroll_waiting,
        SummarySection::Responded => state.summary_scroll_responded,
        SummarySection::Running => state.summary_scroll_running,
        SummarySection::MarkedUnread => state.summary_scroll_marked_unread,
        SummarySection::Idle => state.summary_scroll_idle,
    }
}

pub fn set_section_scroll(state: &mut AppState, section: SummarySection, value: usize) {
    match section {
        SummarySection::Attention => state.summary_scroll_attention = value,
        SummarySection::Waiting => state.summary_scroll_waiting = value,
        SummarySection::Responded => state.summary_scroll_responded = value,
        SummarySection::Running => state.summary_scroll_running = value,
        SummarySection::MarkedUnread => state.summary_scroll_marked_unread = value,
        SummarySection::Idle => state.summary_scroll_idle = value,
    }
}

fn scroll_summary_section(state: &mut AppState, section: SummarySection, delta: isize) {
    let info = section_rect_for(state, section).clone();
    let visible = info.rect.height as usize;
    let max_scroll = info.total_rows.saturating_sub(visible.max(1));
    let cur = section_scroll_get(state, section);
    let next = (cur as isize + delta).max(0) as usize;
    set_section_scroll(state, section, next.min(max_scroll));
}

fn section_has_hidden_below(state: &AppState, section: SummarySection) -> bool {
    let info = section_rect_for(state, section);
    let scroll = section_scroll_get(state, section);
    let visible_count = state
        .layout
        .summary_targets
        .iter()
        .filter(|t| t.section == section)
        .count();
    scroll + visible_count < info.total_rows
}

fn section_has_hidden_above(state: &AppState, section: SummarySection) -> bool {
    section_scroll_get(state, section) > 0
}

fn first_nonempty_section(state: &AppState) -> Option<SummarySection> {
    [
        SummarySection::Attention,
        SummarySection::Waiting,
        SummarySection::Responded,
        SummarySection::Running,
        SummarySection::MarkedUnread,
        SummarySection::Idle,
    ]
    .into_iter()
    .find(|&s| section_rect_for(state, s).total_rows > 0)
}

fn summary_section_at(state: &AppState, row: u16, col: u16) -> Option<SummarySection> {
    [
        SummarySection::Attention,
        SummarySection::Waiting,
        SummarySection::Responded,
        SummarySection::Running,
        SummarySection::MarkedUnread,
        SummarySection::Idle,
    ]
    .into_iter()
    .find(|&s| {
        section_rect_for(state, s)
            .rect
            .contains(ratatui::layout::Position { x: col, y: row })
    })
}
