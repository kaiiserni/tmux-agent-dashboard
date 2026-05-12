use crate::ui::colors::ColorTheme;
use crate::ui::icons::StatusIcons;

pub mod focus;
pub mod layout;
mod refresh;

pub use focus::FocusState;
pub use layout::{FrameLayout, SummarySectionRect, SummaryTarget, TileTarget};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DashboardTab {
    Summary,
    Tiles,
}

/// Identifies a scrollable list section in the dashboard summary view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummarySection {
    Attention,
    Waiting,
    Responded,
    Running,
    MarkedUnread,
    Idle,
}

pub struct AppState {
    pub repo_groups: Vec<crate::group::RepoGroup>,
    pub sort_by_activity: bool,
    pub focus_state: FocusState,
    pub layout: FrameLayout,
    pub tmux_pane: String,
    pub theme: ColorTheme,
    pub icons: StatusIcons,
    pub dashboard_tab: DashboardTab,
    /// Currently-selected tile in the Tiles view (index into `tile_targets`).
    pub tile_selected: usize,
    /// Vertical scroll offset (rows) in the accordion Tiles view.
    /// Adjusted automatically to keep the selected pane visible.
    pub tile_scroll_row: u16,
    /// When true, all panes in the Tiles view render expanded
    /// (vs. only the selected one). Toggled with `f`.
    pub tile_all_expanded: bool,
    /// Currently-selected row in the Summary view (index into `summary_targets`).
    pub summary_selected: usize,
    pub summary_scroll_attention: usize,
    pub summary_scroll_waiting: usize,
    pub summary_scroll_responded: usize,
    pub summary_scroll_running: usize,
    pub summary_scroll_marked_unread: usize,
    pub summary_scroll_idle: usize,
    /// Set by `q` / `Esc` handlers to break out of the event loop cleanly.
    pub should_exit: bool,
    /// Cross-refresh cache of resolved git info per pane path. Keeps
    /// branch labels stable when git transiently fails (lockfile, slow
    /// disk).
    pub git_cache: crate::group::GitInfoCache,
    /// Cached `~/.claude/sessions/*.json` lookup (session_id -> name).
    pub session_names: std::collections::HashMap<String, String>,
    /// Last time `session_names` was rescanned. The scan reruns at most
    /// once every 10 seconds.
    pub session_names_refreshed_at: Option<std::time::Instant>,
}

impl AppState {
    pub fn new(tmux_pane: String) -> Self {
        Self {
            repo_groups: vec![],
            sort_by_activity: true,
            focus_state: FocusState::default(),
            layout: FrameLayout::default(),
            tmux_pane,
            theme: ColorTheme::default(),
            icons: StatusIcons::default(),
            dashboard_tab: DashboardTab::Summary,
            tile_selected: 0,
            tile_scroll_row: 0,
            tile_all_expanded: false,
            summary_selected: 0,
            summary_scroll_attention: 0,
            summary_scroll_waiting: 0,
            summary_scroll_responded: 0,
            summary_scroll_running: 0,
            summary_scroll_marked_unread: 0,
            summary_scroll_idle: 0,
            should_exit: false,
            git_cache: crate::group::GitInfoCache::new(),
            session_names: std::collections::HashMap::new(),
            session_names_refreshed_at: None,
        }
    }
}
