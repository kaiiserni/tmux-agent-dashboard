//! `install-hooks <agent> [--write]` — generate the agent-side hook config that
//! wires `tmux-agent-dashboard hook <agent> <event>` into the agent. The source
//! of truth is each adapter's `HOOK_REGISTRATIONS`, so the config can never drift
//! from what the `hook` subcommand actually handles.
//!
//! Default: print the JSON snippet (safe — paste/merge yourself). `--write`
//! merges it into the agent's config file (backed up first), preserving any
//! other hooks already there (e.g. notify scripts).

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use crate::adapter::HookRegistration;
use crate::adapter::antigravity::AntigravityAdapter;
use crate::adapter::claude::ClaudeAdapter;
use crate::adapter::codex::CodexAdapter;
use crate::adapter::grok::GrokAdapter;
use crate::adapter::pi::PiAdapter;

// opencode is intentionally absent: its adapter exposes no HOOK_REGISTRATIONS
// (it isn't hook-installed here), so install-hooks doesn't support it.
fn registrations(agent: &str) -> Option<&'static [HookRegistration]> {
    match agent {
        "claude" => Some(ClaudeAdapter::HOOK_REGISTRATIONS),
        "codex" => Some(CodexAdapter::HOOK_REGISTRATIONS),
        "antigravity" => Some(AntigravityAdapter::HOOK_REGISTRATIONS),
        "pi" => Some(PiAdapter::HOOK_REGISTRATIONS),
        "grok" => Some(GrokAdapter::HOOK_REGISTRATIONS),
        _ => None,
    }
}

/// Config file to merge into with `--write`. `None` → print-only agent (we don't
/// know its config location/format well enough to edit it safely).
fn config_path(agent: &str) -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").ok()?);
    match agent {
        "claude" => Some(home.join(".claude/settings.json")),
        "codex" => Some(home.join(".codex/hooks.json")),
        // Grok Build reads a directory of hook files (~/.grok/hooks/*.json) in
        // the Claude-compatible `{hooks:{…}}` shape, so a dedicated drop-in file
        // needs no merge with grok's own config.
        "grok" => Some(home.join(".grok/hooks/tmux-agent-dashboard.json")),
        _ => None,
    }
}

fn bin_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "tmux-agent-dashboard".to_string())
}

/// Build `{ Trigger: [ { matcher?, hooks: [{type:command, command}] } ] }` —
/// the shared Claude/Codex hook shape.
pub fn build_hooks(agent: &str, regs: &[HookRegistration], bin: &str) -> Value {
    let mut hooks: Map<String, Value> = Map::new();
    for reg in regs {
        let command = format!("{bin} hook {agent} {}", reg.kind.external_name());
        let mut block = Map::new();
        if let Some(m) = reg.matcher {
            block.insert("matcher".to_string(), json!(m));
        }
        block.insert(
            "hooks".to_string(),
            json!([{ "type": "command", "command": command }]),
        );
        hooks
            .entry(reg.trigger.to_string())
            .or_insert_with(|| Value::Array(vec![]))
            .as_array_mut()
            .expect("trigger value is always an array")
            .push(Value::Object(block));
    }
    Value::Object(hooks)
}

/// True if any block under `existing[trigger]` already runs `command`.
fn already_present(existing: &Value, trigger: &str, command: &str) -> bool {
    existing
        .get(trigger)
        .and_then(|v| v.as_array())
        .is_some_and(|blocks| {
            blocks.iter().any(|b| {
                b.get("hooks")
                    .and_then(|h| h.as_array())
                    .is_some_and(|hs| {
                        hs.iter()
                            .any(|h| h.get("command").and_then(|c| c.as_str()) == Some(command))
                    })
            })
        })
}

/// Merge our generated hooks into an existing `hooks` object, appending only the
/// blocks whose command isn't already there. Returns the number added.
fn merge_into(existing_hooks: &mut Map<String, Value>, generated: &Value, agent: &str, bin: &str) -> usize {
    let mut added = 0;
    let regs = registrations(agent).unwrap_or(&[]);
    for reg in regs {
        let command = format!("{bin} hook {agent} {}", reg.kind.external_name());
        if already_present(&Value::Object(existing_hooks.clone()), reg.trigger, &command) {
            continue;
        }
        // Take the freshly built block for this trigger+matcher from `generated`.
        let Some(block) = generated
            .get(reg.trigger)
            .and_then(|v| v.as_array())
            .and_then(|blocks| {
                blocks.iter().find(|b| {
                    b.get("hooks")
                        .and_then(|h| h.as_array())
                        .and_then(|hs| hs.first())
                        .and_then(|h| h.get("command"))
                        .and_then(|c| c.as_str())
                        == Some(command.as_str())
                })
            })
        else {
            continue;
        };
        existing_hooks
            .entry(reg.trigger.to_string())
            .or_insert_with(|| Value::Array(vec![]))
            .as_array_mut()
            .expect("trigger array")
            .push(block.clone());
        added += 1;
    }
    added
}

pub fn cmd_install_hooks(args: &[String]) -> i32 {
    let Some(agent) = args.first().map(|s| s.as_str()) else {
        eprintln!("usage: tmux-agent-dashboard install-hooks <claude|codex|opencode|antigravity|pi|grok> [--write]");
        return 2;
    };
    let write = args.iter().any(|a| a == "--write");
    let Some(regs) = registrations(agent) else {
        eprintln!("unsupported agent '{agent}' (claude|codex|antigravity|pi|grok)");
        return 2;
    };

    let bin = bin_path();
    let generated = build_hooks(agent, regs, &bin);

    if !write {
        let snippet = json!({ "hooks": generated });
        println!("{}", serde_json::to_string_pretty(&snippet).unwrap_or_default());
        eprintln!("\n# printed only — pass --write to merge into the agent config");
        return 0;
    }

    let Some(path) = config_path(agent) else {
        eprintln!("--write not supported for '{agent}' (unknown config format); use the printed snippet above");
        return 2;
    };

    // Some agents (e.g. grok's ~/.grok/hooks/) keep hooks in a subdir that may
    // not exist yet; create it so the write below can't fail on a missing dir.
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut root: Value = match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_else(|_| json!({})),
        Err(_) => json!({}),
    };
    if !root.is_object() {
        eprintln!("{}: not a JSON object, refusing to write", path.display());
        return 1;
    }

    // Back up before touching it.
    if path.exists() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup = path.with_extension(format!("json.bak-{stamp}"));
        if let Err(e) = std::fs::copy(&path, &backup) {
            eprintln!("backup failed ({e}); aborting");
            return 1;
        }
    }

    let obj = root.as_object_mut().expect("checked object");
    let existing_hooks = obj
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if !existing_hooks.is_object() {
        eprintln!("existing `hooks` is not an object, refusing to write");
        return 1;
    }
    let existing_map = existing_hooks.as_object_mut().expect("checked object");
    let added = merge_into(existing_map, &generated, agent, &bin);

    if let Err(e) = std::fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&root).unwrap_or_default()),
    ) {
        eprintln!("write failed: {e}");
        return 1;
    }
    eprintln!("merged {added} hook(s) into {} (existing hooks preserved)", path.display());
    0
}
