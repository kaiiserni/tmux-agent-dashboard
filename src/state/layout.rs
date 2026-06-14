use ratatui::layout::Rect;

/// Frame-scoped UI output — rewritten by the renderer every frame, read by
/// input handlers (mouse / keyboard) before the next render.
#[derive(Debug, Clone, Default)]
pub struct FrameLayout {
    /// Tile click targets for the Tiles view. Each entry has the rendered
    /// Rect and the pane id the tile represents.
    pub tile_targets: Vec<TileTarget>,
    /// Column count of the tile grid most recently rendered. Used for
    /// j/k row navigation in the dashboard tiles view.
    pub tile_cols: usize,
    /// First and last (inclusive) group index rendered in the tiles view.
    /// Used by input handlers to scroll when selection moves off-screen.
    pub tile_visible_first: usize,
    pub tile_visible_last: usize,
    /// Row click targets for the Summary view's scrollable lists.
    pub summary_targets: Vec<SummaryTarget>,
    /// Inner rect + total row count for each scrollable summary section.
    pub summary_section_attention: SummarySectionRect,
    pub summary_section_waiting: SummarySectionRect,
    pub summary_section_responded: SummarySectionRect,
    pub summary_section_running: SummarySectionRect,
    pub summary_section_marked_unread: SummarySectionRect,
    pub summary_section_idle: SummarySectionRect,
    /// Clickable header items (title bar of the outer block). Each entry
    /// is one action keyword and the x-range it occupies on the border row.
    pub header_targets: Vec<HeaderTarget>,
    /// Clickable pane rows in the Overview tab (visible portion only) — for mouse.
    pub overview_targets: Vec<OverviewTarget>,
    /// All navigable rows in the Overview tab (full list, with absolute row
    /// index) — for keyboard j/k selection and Enter-to-jump.
    pub overview_anchors: Vec<OverviewAnchor>,
    /// Total rendered line count of the Overview tab, for scroll clamping.
    pub overview_total_lines: usize,
    /// Inner height of the Overview tab viewport, for page-scroll steps.
    pub overview_view_height: usize,
    /// Height of the content area while the `/` filter is active, so
    /// Ctrl+u/d in search can step a real half-page.
    pub search_view_height: usize,
}

#[derive(Debug, Clone)]
pub struct OverviewTarget {
    pub rect: Rect,
    pub pane_id: String,
    /// `session:window.pane` from the overview snapshot — fallback when
    /// the pane id has died since the snapshot was taken.
    pub target: String,
}

#[derive(Debug, Clone)]
pub struct OverviewAnchor {
    /// Absolute row index in the full rendered row list (pre-scroll).
    pub row: usize,
    pub pane_id: String,
    pub target: String,
}

/// Action triggered by clicking a header item. Mirrors the matching key
/// in the input handler, except `SwitchTab` (clicking the tab label or
/// "Tab: switch" flips the view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderAction {
    SwitchTab,
    ToggleSort,
    ToggleNames,
    ToggleRespondedOrder,
    ToggleActiveOnly,
    ToggleFold,
    Search,
    ClearSelected,
    ToggleRedact,
    Close,
}

#[derive(Debug, Clone)]
pub struct HeaderTarget {
    pub rect: Rect,
    pub action: HeaderAction,
}

#[derive(Debug, Clone)]
pub struct TileTarget {
    pub rect: Rect,
    pub pane_id: String,
    /// Index of the owning group in `state.repo_groups`. Lets `u`/`d`
    /// figure out which group a tile belongs to in `expand_all` mode.
    pub group_idx: usize,
    /// Grid row across all groups (per-group row offsets continue
    /// monotonically). Used for `j`/`k` row navigation.
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone)]
pub struct SummaryTarget {
    pub rect: Rect,
    pub pane_id: String,
    pub section: super::SummarySection,
}

#[derive(Debug, Clone, Default)]
pub struct SummarySectionRect {
    pub rect: Rect,
    pub total_rows: usize,
}
