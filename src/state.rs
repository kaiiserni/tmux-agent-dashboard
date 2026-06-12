use crate::ui::colors::ColorTheme;
use crate::ui::icons::StatusIcons;

pub mod focus;
pub mod layout;
mod refresh;

pub use focus::FocusState;
pub use layout::{
    FrameLayout, HeaderAction, HeaderTarget, OverviewTarget, SummarySectionRect, SummaryTarget,
    TileTarget,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DashboardTab {
    Summary,
    Tiles,
    Overview,
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
    /// Group index of the first group rendered at the top of the Tiles
    /// view. Bumped when navigation moves selection past the visible
    /// range.
    pub tile_scroll_group: usize,
    /// Name of the single group currently rendered expanded in the
    /// Tiles view. All other groups show only their header line.
    /// `None` = every group is folded (manual collapse-all). On first
    /// render with `None`, the first non-empty group is auto-expanded.
    pub expanded_group: Option<String>,
    /// When `true`, every non-empty group renders expanded simultaneously.
    /// Reached by cycling `f`: single → all-folded → all-expanded → single.
    /// Overrides `expanded_group`; navigation works across all tiles.
    pub expand_all_groups: bool,
    /// Tiles view: when `true`, hide pure-idle panes (idle, no attention,
    /// no marked-unread). Toggled with `a` in the Tiles tab.
    pub tiles_hide_idle: bool,
    /// Currently-selected row in the Summary view (index into `summary_targets`).
    pub summary_selected: usize,
    pub summary_scroll_attention: usize,
    pub summary_scroll_waiting: usize,
    pub summary_scroll_responded: usize,
    pub summary_scroll_running: usize,
    pub summary_scroll_marked_unread: usize,
    pub summary_scroll_idle: usize,
    /// Screenshot/redact mode (`<`): hides age + activity-timestamp
    /// columns and masks all free-text content (repo, branch, reason,
    /// prompt, group headers, activity labels).
    pub privacy_mode: bool,
    /// When `true`, force the technical view (repo + branch) regardless
    /// of whether `@pane_name` is set. Loaded from / persisted to the
    /// tmux global option `@dashboard_show_technical_names`.
    pub show_technical_names: bool,
    /// When `true`, the Responded section is sorted newest-first instead
    /// of the default oldest-first review-queue order. Loaded from /
    /// persisted to the tmux global option
    /// `@dashboard_responded_newest_first`.
    pub responded_newest_first: bool,
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
    /// Per-refresh lookup of each pane's most recent activity-log entry,
    /// keyed by pane id. Drives the per-tile context-preview line.
    pub last_activity: std::collections::HashMap<String, crate::activity::ActivityEntry>,
    /// Parsed agent-overview snapshot for the Overview tab. Reloaded on
    /// every refresh tick; `None` when the job never produced output.
    pub overview: Option<crate::overview::Overview>,
    /// Scroll offset (rows) of the Overview tab.
    pub overview_scroll: usize,
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
            tile_scroll_group: 0,
            expanded_group: None,
            expand_all_groups: true,
            tiles_hide_idle: false,
            summary_selected: 0,
            summary_scroll_attention: 0,
            summary_scroll_waiting: 0,
            summary_scroll_responded: 0,
            summary_scroll_running: 0,
            summary_scroll_marked_unread: 0,
            summary_scroll_idle: 0,
            privacy_mode: false,
            show_technical_names: crate::tmux::get_option(
                crate::tmux::DASHBOARD_SHOW_TECHNICAL_NAMES,
            )
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
            responded_newest_first: crate::tmux::get_option(
                crate::tmux::DASHBOARD_RESPONDED_NEWEST_FIRST,
            )
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false),
            should_exit: false,
            git_cache: crate::group::GitInfoCache::new(),
            session_names: std::collections::HashMap::new(),
            session_names_refreshed_at: None,
            last_activity: std::collections::HashMap::new(),
            overview: crate::overview::load(),
            overview_scroll: 0,
        }
    }
}
