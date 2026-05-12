use indexmap::IndexMap;

use crate::tmux::PaneInfo;

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
    pub name: String,
    pub has_focus: bool,
    pub panes: Vec<(PaneInfo, PaneGitInfo)>,
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

pub fn group_panes_by_repo(sessions: &[crate::tmux::SessionInfo]) -> Vec<RepoGroup> {
    let mut groups: IndexMap<String, RepoGroup> = IndexMap::new();
    let mut git_cache: std::collections::HashMap<String, PaneGitInfo> =
        std::collections::HashMap::new();

    for session in sessions {
        for window in &session.windows {
            for pane in &window.panes {
                let mut git_info = match git_cache.get(pane.path.as_str()) {
                    Some(cached) => cached.clone(),
                    None => {
                        let resolved = resolve_pane_git_info(&pane.path);
                        git_cache.insert(pane.path.clone(), resolved.clone());
                        resolved
                    }
                };

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

                let group = groups.entry(group_key).or_insert_with(|| RepoGroup {
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
