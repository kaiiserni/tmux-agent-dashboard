//! Process-tree agent detection — the fallback for panes that have no
//! `@pane_agent` set (e.g. a quick `codex` run with no hooks installed).
//! Ported from tmux-agent-sidebar: one `ps` snapshot, walk the tree under a
//! pane's pid, and see if a known agent binary is a descendant.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::process::Command;

use crate::tmux::{
    ANTIGRAVITY_AGENT, AgentType, CLAUDE_AGENT, CODEX_AGENT, OPENCODE_AGENT, PI_AGENT,
};

/// Agent binary names tried against the process tree, paired with their type.
const AGENT_NAMES: &[(&str, AgentType)] = &[
    (CLAUDE_AGENT, AgentType::Claude),
    (CODEX_AGENT, AgentType::Codex),
    (OPENCODE_AGENT, AgentType::OpenCode),
    (ANTIGRAVITY_AGENT, AgentType::Antigravity),
    (PI_AGENT, AgentType::Pi),
];

#[derive(Debug, Clone)]
struct ProcessInfo {
    comm: String,
    args: String,
}

pub struct ProcessSnapshot {
    children_of: HashMap<u32, Vec<u32>>,
    info_by_pid: HashMap<u32, ProcessInfo>,
}

impl ProcessSnapshot {
    /// One `ps` snapshot of every process. `None` if `ps` fails.
    pub fn scan() -> Option<Self> {
        let output = Command::new("ps")
            .args(["-eo", "pid=,ppid=,comm=,args="])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(Self::from_ps_output(&String::from_utf8_lossy(&output.stdout)))
    }

    fn from_ps_output(ps_output: &str) -> Self {
        let mut children_of: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut info_by_pid: HashMap<u32, ProcessInfo> = HashMap::new();
        for line in ps_output.lines() {
            let mut parts = line.split_whitespace();
            let (Some(pid), Some(ppid), Some(comm)) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let (Ok(pid), Ok(ppid)) = (pid.parse::<u32>(), ppid.parse::<u32>()) else {
                continue;
            };
            children_of.entry(ppid).or_default().push(pid);
            info_by_pid.insert(
                pid,
                ProcessInfo {
                    comm: comm.to_string(),
                    args: parts.collect::<Vec<_>>().join(" "),
                },
            );
        }
        Self { children_of, info_by_pid }
    }

    fn descendants(&self, seed: u32) -> HashSet<u32> {
        let mut seen = HashSet::new();
        let mut queue: VecDeque<u32> = VecDeque::from([seed]);
        while let Some(pid) = queue.pop_front() {
            if !seen.insert(pid) {
                continue;
            }
            if let Some(children) = self.children_of.get(&pid) {
                queue.extend(children.iter().copied());
            }
        }
        seen
    }

    /// The first known agent whose binary runs anywhere in the tree under
    /// `pane_pid`, or `None`.
    pub fn detect_agent(&self, pane_pid: u32) -> Option<AgentType> {
        let tree = self.descendants(pane_pid);
        for &pid in &tree {
            let Some(info) = self.info_by_pid.get(&pid) else {
                continue;
            };
            for (name, agent) in AGENT_NAMES {
                if process_matches_agent(info, name) {
                    return Some(agent.clone());
                }
            }
        }
        None
    }
}

fn command_basename(command: &str) -> &str {
    Path::new(command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(command)
}

fn process_matches_agent(info: &ProcessInfo, agent_name: &str) -> bool {
    if command_basename(&info.comm) == agent_name {
        return true;
    }
    info.args
        .split_whitespace()
        .next()
        .is_some_and(|c| command_basename(c.trim_matches('"')) == agent_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(lines: &str) -> ProcessSnapshot {
        ProcessSnapshot::from_ps_output(lines)
    }

    #[test]
    fn detects_codex_descendant() {
        // pane shell 100 → node 200 → codex 300
        let s = snap("100 1 zsh -zsh\n200 100 node node\n300 200 codex /opt/codex\n");
        assert_eq!(s.detect_agent(100), Some(AgentType::Codex));
    }

    #[test]
    fn no_agent_in_tree() {
        let s = snap("100 1 zsh -zsh\n200 100 vim vim file\n");
        assert_eq!(s.detect_agent(100), None);
    }

    #[test]
    fn matches_first_arg_basename() {
        let s = snap("100 1 node /usr/local/bin/claude --foo\n");
        assert_eq!(s.detect_agent(100), Some(AgentType::Claude));
    }
}
