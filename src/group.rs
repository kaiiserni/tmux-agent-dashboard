use std::collections::HashMap;
use std::time::{Duration, Instant};

use indexmap::IndexMap;

use crate::tmux::{PaneInfo, PaneStatus};

/// A pane is "visible" under the Tiles hide-idle filter unless it is pure
/// idle: not running, not flagged for attention, not marked unread.
pub fn pane_is_visible(pane: &PaneInfo) -> bool {
    !matches!(pane.status, PaneStatus::Idle) || pane.attention || pane.marked_unread_at.is_some()
}

/// Free-text haystack for the dashboard's `/` fuzzy filter: group name plus
/// every label a pane might be recognised by, across both display modes.
/// The agent vendor name is deliberately excluded — it's identical across
/// most panes ("claude"), so it would match every short query ("cl").
fn pane_haystack(name: &str, p: &PaneInfo, info: &PaneGitInfo) -> String {
    let branch = info.branch.as_deref().unwrap_or("");
    format!(
        "{name} {} {} {} {} {} {} {}",
        p.tmux_session_name,
        p.session_name,
        p.pane_name,
        p.window_name,
        p.worktree.branch,
        p.worktree.name,
        branch,
    )
}

/// Whether a pane matches the active search query. Empty query matches all,
/// so callers can pass the query unconditionally.
pub fn pane_matches_search(name: &str, p: &PaneInfo, info: &PaneGitInfo, search: &str) -> bool {
    search.is_empty() || crate::fuzzy::score(&pane_haystack(name, p, info), search).is_some()
}

/// Max age before a cached `PaneGitInfo` entry is re-resolved.
/// Refresh fires every second; without this the dashboard would shell out
/// to `git` for every pane on every tick.
const GIT_CACHE_TTL: Duration = Duration::from_secs(30);

/// Per-path cache of resolved git info. Keyed by pane path.
pub type GitInfoCache = HashMap<String, (PaneGitInfo, Instant)>;

/// Per-pane git metadata resolved from the pane's working directory.
/// The dashboard reads this in a single batch (one git call per unique
/// path); the result feeds the per-row branch label fallback when the
/// session name isn't set.
#[derive(Debug, Clone, Default)]
pub struct PaneGitInfo {
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub worktree_name: Option<String>,
}

/// A group of panes working in the same repository (or directory).
#[derive(Debug, Clone)]
pub struct RepoGroup {
    /// Unique identifier (repo_root path or fallback pane path). Two
    /// groups can share `name` (e.g. multiple bare-repo worktrees all
    /// resolve to `…/main`), so `key` is what we use for identity.
    pub key: String,
    pub name: String,
    pub has_focus: bool,
    pub panes: Vec<(PaneInfo, PaneGitInfo)>,
}

impl RepoGroup {
    /// Indices into `panes` rendered in the Tiles view under the current
    /// filter. With `hide_idle`, pure-idle panes are dropped; a non-empty
    /// `search` also drops panes that don't match the fuzzy query.
    pub fn visible_pane_indices(&self, hide_idle: bool, search: &str) -> Vec<usize> {
        self.panes
            .iter()
            .enumerate()
            .filter(|(_, (p, info))| {
                (!hide_idle || pane_is_visible(p))
                    && pane_matches_search(&self.name, p, info, search)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Number of panes the Tiles view shows for this group.
    pub fn visible_pane_count(&self, hide_idle: bool, search: &str) -> usize {
        self.panes
            .iter()
            .filter(|(p, info)| {
                (!hide_idle || pane_is_visible(p))
                    && pane_matches_search(&self.name, p, info, search)
            })
            .count()
    }

    /// Whether this group has any pane the Tiles view would show.
    pub fn has_visible_panes(&self, hide_idle: bool, search: &str) -> bool {
        self.panes.iter().any(|(p, info)| {
            (!hide_idle || pane_is_visible(p)) && pane_matches_search(&self.name, p, info, search)
        })
    }
}

/// Single git call for branch + repo root.
fn resolve_pane_git_info(path: &str) -> PaneGitInfo {
    if path.is_empty() {
        return PaneGitInfo::default();
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--abbrev-ref", "HEAD", "--git-common-dir"])
        .output()
        .ok();
    let combined = match output {
        Some(o) if o.status.success() => Some(String::from_utf8_lossy(&o.stdout).to_string()),
        _ => None,
    };
    let (branch, git_common_dir) = match combined {
        Some(s) => {
            let mut lines = s.lines();
            (
                lines.next().map(str::to_string),
                lines.next().map(str::to_string),
            )
        }
        None => (None, None),
    };

    let repo_root = git_common_dir.as_ref().and_then(|common| {
        let abs = if std::path::Path::new(common).is_absolute() {
            std::path::PathBuf::from(common)
        } else {
            std::path::PathBuf::from(path).join(common)
        };
        if abs.file_name().map(|n| n == ".git").unwrap_or(false) {
            abs.parent().map(|p| p.to_string_lossy().to_string())
        } else {
            Some(abs.to_string_lossy().to_string())
        }
    });

    PaneGitInfo {
        repo_root,
        branch,
        worktree_name: None,
    }
}

/// Returns true when the newly-resolved info is a "better" value than what
/// we previously had cached. Used to keep a known-good branch label even
/// when git transiently returns nothing (lockfile contention, slow disk,
/// etc.). Without this, the per-row label visibly cycles between the
/// session name, the branch, and the path basename across refreshes.
fn is_better_git_info(new: &PaneGitInfo, old: &PaneGitInfo) -> bool {
    let new_has_branch = new.branch.as_deref().is_some_and(|b| !b.trim().is_empty());
    let old_has_branch = old.branch.as_deref().is_some_and(|b| !b.trim().is_empty());
    if new_has_branch && !old_has_branch {
        return true;
    }
    if !new_has_branch && old_has_branch {
        return false;
    }
    // Both have (or both lack) a branch; prefer the newer if it now also
    // resolves a repo_root that we didn't have before.
    let new_has_root = new.repo_root.as_deref().is_some_and(|r| !r.is_empty());
    let old_has_root = old.repo_root.as_deref().is_some_and(|r| !r.is_empty());
    new_has_root || !old_has_root
}

/// Look up `path` in the cache, refreshing it from `git` if the entry is
/// missing or stale. Honors the sticky rule from `is_better_git_info`.
fn cached_git_info(cache: &mut GitInfoCache, path: &str) -> PaneGitInfo {
    let now = Instant::now();
    if let Some((info, stamp)) = cache.get(path)
        && now.duration_since(*stamp) < GIT_CACHE_TTL
    {
        return info.clone();
    }
    let fresh = resolve_pane_git_info(path);
    match cache.get(path).cloned() {
        Some((old, _)) => {
            if is_better_git_info(&fresh, &old) {
                cache.insert(path.to_string(), (fresh.clone(), now));
                fresh
            } else {
                // Keep the known-good value but refresh the timestamp so
                // we don't re-shell every tick when git keeps failing.
                cache.insert(path.to_string(), (old.clone(), now));
                old
            }
        }
        None => {
            cache.insert(path.to_string(), (fresh.clone(), now));
            fresh
        }
    }
}

/// Group panes by repository, reusing entries from the supplied cache.
/// Callers that don't have access to long-lived state should use
/// [`group_panes_by_repo`] instead.
pub fn group_panes_with_cache(
    sessions: &[crate::tmux::SessionInfo],
    cache: &mut GitInfoCache,
) -> Vec<RepoGroup> {
    let mut groups: IndexMap<String, RepoGroup> = IndexMap::new();

    for session in sessions {
        for window in &session.windows {
            for pane in &window.panes {
                let mut git_info = cached_git_info(cache, &pane.path);

                if !pane.worktree.name.is_empty() {
                    git_info.worktree_name = Some(pane.worktree.name.clone());
                }
                if !pane.worktree.branch.is_empty() {
                    git_info.branch = Some(pane.worktree.branch.clone());
                }

                let group_key = match &git_info.repo_root {
                    Some(root) => root.clone(),
                    None => pane.path.clone(),
                };

                let display_name = group_key
                    .rsplit('/')
                    .next()
                    .unwrap_or(&group_key)
                    .to_string();

                let has_focus = window.window_active && pane.pane_active;

                let group = groups
                    .entry(group_key.clone())
                    .or_insert_with(|| RepoGroup {
                        key: group_key.clone(),
                        name: display_name,
                        has_focus: false,
                        panes: Vec::new(),
                    });

                if has_focus {
                    group.has_focus = true;
                }

                group.panes.push((pane.clone(), git_info));
            }
        }
    }

    let mut result: Vec<RepoGroup> = groups.into_values().collect();
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    result
}

/// One-shot grouping for callers without long-lived state (e.g. the
/// `pending` CLI subcommand). Uses a throwaway cache so a single
/// invocation still avoids duplicate git calls across panes that share
/// a path.
pub fn group_panes_by_repo(sessions: &[crate::tmux::SessionInfo]) -> Vec<RepoGroup> {
    let mut cache = GitInfoCache::new();
    group_panes_with_cache(sessions, &mut cache)
}
