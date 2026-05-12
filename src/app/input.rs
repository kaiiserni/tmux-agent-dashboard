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
                state.tile_scroll_row = state.tile_scroll_row.saturating_add(2);
                return true;
            }
            MouseEventKind::ScrollUp if state.dashboard_tab == DashboardTab::Tiles => {
                state.tile_scroll_row = state.tile_scroll_row.saturating_sub(2);
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

// ─── Tiles tab navigation (accordion) ───────────────────────────────

fn handle_dashboard_tiles_key(state: &mut AppState, code: KeyCode) -> bool {
    // tile_targets holds every pane in render order (row index == pane
    // index across groups). Navigation is purely linear; the accordion
    // renderer handles scroll + expansion automatically.
    let total_targets = state.layout.tile_targets.len();
    // Use repo_groups as the source of truth: tile_targets only
    // contains panes that were visible this frame, but selection is a
    // global index across all panes.
    let total_panes: usize = state.repo_groups.iter().map(|g| g.panes.len()).sum();
    if total_panes == 0 {
        return false;
    }
    let cur = state.tile_selected.min(total_panes - 1);

    match code {
        KeyCode::Char('j') | KeyCode::Down => {
            if cur + 1 < total_panes {
                state.tile_selected = cur + 1;
            }
            true
        }
        KeyCode::Char('k') | KeyCode::Up => {
            if cur > 0 {
                state.tile_selected = cur - 1;
            }
            true
        }
        KeyCode::Char('h') | KeyCode::Left => {
            // Jump to first pane of the previous non-empty group.
            if let Some(idx) = first_pane_of_adjacent_group(state, cur, false) {
                state.tile_selected = idx;
            }
            true
        }
        KeyCode::Char('l') | KeyCode::Right => {
            // Jump to first pane of the next non-empty group.
            if let Some(idx) = first_pane_of_adjacent_group(state, cur, true) {
                state.tile_selected = idx;
            }
            true
        }
        KeyCode::PageDown => {
            state.tile_selected = (cur + 5).min(total_panes - 1);
            true
        }
        KeyCode::PageUp => {
            state.tile_selected = cur.saturating_sub(5);
            true
        }
        KeyCode::Char('g') => {
            state.tile_selected = 0;
            true
        }
        KeyCode::Char('G') => {
            state.tile_selected = total_panes - 1;
            true
        }
        KeyCode::Char('m') => {
            // Toggle marked-unread on the currently selected pane.
            if let Some(pane_id) = pane_id_for_flat_index(state, cur) {
                toggle_tile_mark(state, &pane_id);
            }
            true
        }
        KeyCode::Char('f') => {
            // Fold / unfold every pane in the Tiles view.
            state.tile_all_expanded = !state.tile_all_expanded;
            true
        }
        KeyCode::Enter => {
            if let Some(target) = state.layout.tile_targets.get(cur).cloned() {
                state.activate_pane_by_id(&target.pane_id);
                state.should_exit = true;
            } else if total_targets > 0 {
                // Fallback: selected pane scrolled off-screen — still activate.
                if let Some(pane_id) = pane_id_for_flat_index(state, cur) {
                    state.activate_pane_by_id(&pane_id);
                    state.should_exit = true;
                }
            }
            true
        }
        _ => false,
    }
}

fn pane_id_for_flat_index(state: &AppState, idx: usize) -> Option<String> {
    let mut acc = 0usize;
    for group in &state.repo_groups {
        if idx < acc + group.panes.len() {
            let p_idx = idx - acc;
            return Some(group.panes[p_idx].0.pane_id.clone());
        }
        acc += group.panes.len();
    }
    None
}

/// Returns the flat pane index of the first pane in the adjacent group
/// (`forward=true` → next group, else previous).
fn first_pane_of_adjacent_group(state: &AppState, cur: usize, forward: bool) -> Option<usize> {
    // Find the (group_idx, group_offset) for the current selection.
    let mut acc = 0usize;
    let mut cur_group = 0usize;
    for (g_idx, group) in state.repo_groups.iter().enumerate() {
        if cur < acc + group.panes.len() {
            cur_group = g_idx;
            break;
        }
        acc += group.panes.len();
    }

    let group_range: Vec<usize> = if forward {
        ((cur_group + 1)..state.repo_groups.len()).collect()
    } else {
        (0..cur_group).rev().collect()
    };
    let mut offset = 0usize;
    if !forward {
        // Compute offsets for previous groups.
        for g in 0..cur_group {
            offset += state.repo_groups[g].panes.len();
        }
    }
    for g in group_range {
        if !forward {
            offset = offset.saturating_sub(state.repo_groups[g].panes.len());
        }
        if !state.repo_groups[g].panes.is_empty() {
            return Some(if forward {
                acc + state.repo_groups[cur_group].panes.len()
                    + state
                        .repo_groups
                        .iter()
                        .skip(cur_group + 1)
                        .take(g - cur_group - 1)
                        .map(|gp| gp.panes.len())
                        .sum::<usize>()
            } else {
                offset
            });
        }
        if forward {
            // forward variant: simply find the first non-empty group after
            // current and compute its starting flat index.
            let start: usize = state
                .repo_groups
                .iter()
                .take(g)
                .map(|gp| gp.panes.len())
                .sum();
            return Some(start);
        }
    }
    None
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
