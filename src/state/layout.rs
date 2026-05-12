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
}

#[derive(Debug, Clone)]
pub struct TileTarget {
    pub rect: Rect,
    pub pane_id: String,
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
