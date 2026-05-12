use ratatui::style::Color;

use crate::tmux::{self, AgentType, PaneStatus};

/// Runtime color theme. Reads the same `@sidebar_color_*` tmux options
/// the sidebar plugin documents so user customisation carries over.
#[derive(Debug, Clone)]
pub struct ColorTheme {
    pub accent: Color,
    pub border_inactive: Color,
    pub status_all: Color,
    pub status_running: Color,
    pub status_waiting: Color,
    pub status_idle: Color,
    pub status_error: Color,
    pub status_unknown: Color,
    pub filter_inactive: Color,
    pub agent_claude: Color,
    pub agent_codex: Color,
    pub agent_opencode: Color,
    pub text_active: Color,
    pub text_muted: Color,
    pub text_inactive: Color,
    pub session_header: Color,
    pub wait_reason: Color,
    pub selection_bg: Color,
    pub branch: Color,
    pub badge_danger: Color,
    pub badge_auto: Color,
    pub badge_plan: Color,
    pub section_title: Color,
    pub activity_timestamp: Color,
    pub response_arrow: Color,
}

impl Default for ColorTheme {
    fn default() -> Self {
        Self {
            accent: Color::Indexed(153),
            border_inactive: Color::Indexed(240),
            status_all: Color::Indexed(111),
            status_running: Color::Indexed(114),
            status_waiting: Color::Indexed(221),
            status_idle: Color::Indexed(110),
            status_error: Color::Indexed(167),
            status_unknown: Color::Indexed(244),
            filter_inactive: Color::Indexed(245),
            agent_claude: Color::Indexed(174),
            agent_codex: Color::Indexed(141),
            agent_opencode: Color::Indexed(117),
            text_active: Color::Indexed(255),
            text_muted: Color::Indexed(252),
            text_inactive: Color::Indexed(244),
            session_header: Color::Indexed(39),
            wait_reason: Color::Indexed(221),
            selection_bg: Color::Indexed(239),
            branch: Color::Indexed(109),
            badge_danger: Color::Indexed(167),
            badge_auto: Color::Indexed(221),
            badge_plan: Color::Indexed(117),
            section_title: Color::Indexed(109),
            activity_timestamp: Color::Indexed(109),
            response_arrow: Color::Indexed(81),
        }
    }
}

impl ColorTheme {
    pub fn from_tmux() -> Self {
        let mut theme = Self::default();
        let all_opts = tmux::get_all_global_options();

        let read = |var: &str, fallback: Color| -> Color {
            all_opts
                .get(var)
                .and_then(|s| s.parse::<u8>().ok())
                .map(Color::Indexed)
                .unwrap_or(fallback)
        };

        theme.accent = read(tmux::SIDEBAR_COLOR_ACCENT, theme.accent);
        theme.border_inactive = read(tmux::SIDEBAR_COLOR_BORDER, theme.border_inactive);
        theme.status_all = read(tmux::SIDEBAR_COLOR_ALL, theme.status_all);
        theme.status_running = read(tmux::SIDEBAR_COLOR_RUNNING, theme.status_running);
        theme.status_waiting = read(tmux::SIDEBAR_COLOR_WAITING, theme.status_waiting);
        theme.status_idle = read(tmux::SIDEBAR_COLOR_IDLE, theme.status_idle);
        theme.status_error = read(tmux::SIDEBAR_COLOR_ERROR, theme.status_error);
        theme.filter_inactive = read(tmux::SIDEBAR_COLOR_FILTER_INACTIVE, theme.filter_inactive);
        theme.agent_claude = read(tmux::SIDEBAR_COLOR_AGENT_CLAUDE, theme.agent_claude);
        theme.agent_codex = read(tmux::SIDEBAR_COLOR_AGENT_CODEX, theme.agent_codex);
        theme.agent_opencode = read(tmux::SIDEBAR_COLOR_AGENT_OPENCODE, theme.agent_opencode);
        theme.text_active = read(tmux::SIDEBAR_COLOR_TEXT_ACTIVE, theme.text_active);
        theme.text_muted = read(tmux::SIDEBAR_COLOR_TEXT_MUTED, theme.text_muted);
        theme.text_inactive = read(tmux::SIDEBAR_COLOR_TEXT_INACTIVE, theme.text_inactive);
        theme.session_header = read(tmux::SIDEBAR_COLOR_SESSION, theme.session_header);
        theme.wait_reason = read(tmux::SIDEBAR_COLOR_WAIT_REASON, theme.wait_reason);
        theme.selection_bg = read(tmux::SIDEBAR_COLOR_SELECTION, theme.selection_bg);
        theme.branch = read(tmux::SIDEBAR_COLOR_BRANCH, theme.branch);
        theme.badge_danger = read(tmux::SIDEBAR_COLOR_BADGE_DANGER, theme.badge_danger);
        theme.badge_auto = read(tmux::SIDEBAR_COLOR_BADGE_AUTO, theme.badge_auto);
        theme.badge_plan = read(tmux::SIDEBAR_COLOR_BADGE_PLAN, theme.badge_plan);
        theme.section_title = read(tmux::SIDEBAR_COLOR_SECTION_TITLE, theme.section_title);
        theme.activity_timestamp = read(
            tmux::SIDEBAR_COLOR_ACTIVITY_TIMESTAMP,
            theme.activity_timestamp,
        );
        theme.response_arrow = read(tmux::SIDEBAR_COLOR_RESPONSE_ARROW, theme.response_arrow);

        theme
    }

    pub fn agent_color(&self, agent: &AgentType) -> Color {
        match agent {
            AgentType::Claude => self.agent_claude,
            AgentType::Codex => self.agent_codex,
            AgentType::OpenCode => self.agent_opencode,
            AgentType::Unknown => self.status_unknown,
        }
    }

    pub fn status_color(&self, status: &PaneStatus) -> Color {
        match status {
            PaneStatus::Running | PaneStatus::Background => self.status_running,
            PaneStatus::Waiting => self.status_waiting,
            PaneStatus::Idle => self.status_idle,
            PaneStatus::Error => self.status_error,
            PaneStatus::Unknown => self.status_unknown,
        }
    }
}
