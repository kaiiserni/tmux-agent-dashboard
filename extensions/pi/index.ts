import { spawn, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

type ExtensionAPI = { on: (event: string, handler: (ev: any, ctx: any) => any) => void };

function resolveBinary(): string {
  const envVar = process.env.TMUX_AGENT_DASHBOARD_BIN;
  if (envVar && existsSync(envVar)) return envVar;

  const home = homedir();
  const candidates = [
    join(home, ".tmux/plugins/tmux-agent-dashboard/bin/tmux-agent-dashboard"),
    join(home, ".tmux/plugins/tmux-agent-dashboard/target/release/tmux-agent-dashboard"),
    join(home, "projects/tmux-agent-dashboard/bin/tmux-agent-dashboard"),
    join(home, "projects/tmux-agent-dashboard/target/release/tmux-agent-dashboard"),
  ];
  for (const c of candidates) if (existsSync(c)) return c;

  const which = spawnSync("command", ["-v", "tmux-agent-dashboard"], { shell: true });
  const found = which.stdout?.toString().trim();
  if (found) return found;

  return "tmux-agent-dashboard";
}

const BINARY = resolveBinary();

function emit(eventName: string, payload: Record<string, unknown>): void {
  try {
    const child = spawn(BINARY, ["hook", "pi", eventName], {
      stdio: ["pipe", "ignore", "ignore"],
      detached: false,
    });
    child.on("error", () => {});
    child.stdin.end(JSON.stringify(payload));
  } catch {
    // Never let dashboard plumbing break the Pi session.
  }
}

function safeString(v: unknown): string {
  if (typeof v === "string") return v;
  if (v == null) return "";
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}

export default function (pi: ExtensionAPI) {
  pi.on("session_start", async (event: any, ctx: any) => {
    emit("session-start", {
      cwd: ctx?.cwd ?? process.cwd(),
      source: event?.reason ?? "startup",
      session_id: ctx?.sessionId ?? ctx?.sessionManager?.sessionId,
    });
  });

  pi.on("before_agent_start", async (event: any, ctx: any) => {
    const prompt = safeString(event?.prompt);
    if (!prompt) return;
    emit("user-prompt-submit", {
      cwd: ctx?.cwd ?? process.cwd(),
      prompt,
      session_id: ctx?.sessionId ?? ctx?.sessionManager?.sessionId,
    });
  });

  pi.on("tool_call", async (event: any, _ctx: any) => {
    const tool = safeString(event?.toolName);
    if (!tool) return;
    emit("activity-log", {
      tool_name: tool,
      tool_input: event?.input ?? null,
    });
  });

  pi.on("tool_result", async (event: any, _ctx: any) => {
    const tool = safeString(event?.toolName);
    if (!tool) return;
    emit("activity-log", {
      tool_name: tool,
      tool_input: event?.input ?? null,
      tool_response: event?.content ?? event?.details ?? null,
    });
  });

  pi.on("agent_end", async (_event: any, ctx: any) => {
    emit("stop", {
      cwd: ctx?.cwd ?? process.cwd(),
      last_message: "",
      session_id: ctx?.sessionId ?? ctx?.sessionManager?.sessionId,
    });
  });
}
