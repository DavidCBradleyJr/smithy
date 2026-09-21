// Folds a thread's transcript events (as recorded by the backend) into the
// items the transcript renders. Used for both live streaming and history.

export type ToolStatus = "pending" | "in_progress" | "completed" | "failed";

export interface DiffBlock {
  path: string;
  oldText: string | null;
  newText: string;
}

export interface ToolItem {
  kind: "tool";
  id: string;
  title: string;
  toolKind: string;
  status: ToolStatus;
  text: string[];
  diffs: DiffBlock[];
  output: string;
  exitCode: number | null;
  rawInput: unknown;
  paths: string[];
  started: number;
  ended: number | null;
}

export interface PermissionOption {
  optionId: string;
  name: string;
  kind: string;
}

export type Item =
  | { kind: "user"; text: string; at: number }
  | { kind: "text"; text: string }
  | { kind: "thought"; text: string; started: number; ended: number | null }
  | ToolItem
  | {
      kind: "permission";
      key: string;
      title: string;
      options: PermissionOption[];
      chosen: string | null | undefined; // undefined = still waiting
    }
  | { kind: "plan"; entries: { content: string; status: string; priority?: string }[] }
  | { kind: "stop"; reason: string; at: number; took: number | null }
  | { kind: "error"; message: string }
  | { kind: "note"; text: string };

export interface ConfigOption {
  id: string;
  name: string;
  category?: string;
  currentValue: string;
  options: { value: string; name: string; description?: string | null }[];
}

export interface ThreadView {
  items: Item[];
  config: ConfigOption[];
  agentName: string | null;
  /** A turn is in flight: last user prompt has no stop/error after it. */
  busy: boolean;
  waiting: boolean;
}

export interface RawEvent {
  t: string;
  at?: number;
  [k: string]: any;
}

/** Pi's own retry chatter, sent as if it were the answer. */
export function isRetryNotice(text: string) {
  const t = text.trim();
  return (t.startsWith("Retrying (attempt ") && t.endsWith("...")) || t === "Retry finished, resuming.";
}

function contentText(c: any): string {
  if (!c) return "";
  if (c.type === "text") return c.text ?? "";
  if (c.type === "content") return contentText(c.content);
  return "";
}

function applyToolFields(item: ToolItem, u: any, at: number) {
  if (u.title) item.title = u.title;
  if (u.kind) item.toolKind = u.kind;
  if (u.status) {
    item.status = u.status;
    if ((u.status === "completed" || u.status === "failed") && item.ended == null) item.ended = at;
  }
  if (u.rawInput !== undefined) item.rawInput = u.rawInput;
  if (Array.isArray(u.locations)) item.paths = u.locations.map((l: any) => l.path).filter(Boolean);
  if (Array.isArray(u.content)) {
    // `content` replaces the tool's content wholesale per ACP.
    item.text = [];
    item.diffs = [];
    for (const c of u.content) {
      if (c.type === "diff") item.diffs.push({ path: c.path, oldText: c.oldText ?? null, newText: c.newText ?? "" });
      else if (c.type === "content") item.text.push(contentText(c));
    }
  }
  // Terminal output as streamed by pi-acp and Zed-style adapters.
  const meta = u._meta ?? {};
  if (meta.terminal_output?.data) item.output += meta.terminal_output.data;
  if (meta.terminal_exit) item.exitCode = meta.terminal_exit.exit_code ?? null;
  if (u.rawOutput !== undefined && !item.output && typeof u.rawOutput?.output === "string") item.output = u.rawOutput.output;
}

export function reduce(events: RawEvent[]): ThreadView {
  const items: Item[] = [];
  const tools = new Map<string, ToolItem>();
  let config: ConfigOption[] = [];
  let agentName: string | null = null;
  let busy = false;
  let turnStart = 0;

  const last = () => items[items.length - 1];
  const closeThought = (at: number) => {
    const l = last();
    if (l?.kind === "thought" && l.ended == null) l.ended = at;
  };

  for (const e of events) {
    const at = e.at ?? 0;
    switch (e.t) {
      case "session":
        if (Array.isArray(e.configOptions)) config = e.configOptions;
        agentName = e.agentInfo?.title ?? e.agentInfo?.name ?? agentName;
        if (e.resumed === false && items.length > 0) {
          items.push({ kind: "note", text: "Agent restarted with a fresh session; it won't remember earlier turns." });
        }
        break;
      case "user":
        items.push({ kind: "user", text: e.text, at });
        busy = true;
        turnStart = at;
        break;
      case "stop":
        closeThought(at);
        items.push({ kind: "stop", reason: e.reason, at, took: turnStart ? at - turnStart : null });
        busy = false;
        break;
      case "error":
        items.push({ kind: "error", message: e.message });
        busy = false;
        break;
      case "exit":
        if (busy) items.push({ kind: "error", message: "The agent process exited." });
        busy = false;
        break;
      case "permission":
        items.push({
          kind: "permission",
          key: e.key,
          title: e.toolCall?.title ?? "Tool call",
          options: e.options ?? [],
          chosen: undefined,
        });
        break;
      case "permission_resolved": {
        const p = items.find((i) => i.kind === "permission" && i.key === e.key);
        if (p && p.kind === "permission") p.chosen = e.optionId ?? null;
        break;
      }
      case "update": {
        const u = e.update ?? {};
        switch (u.sessionUpdate) {
          case "agent_message_chunk": {
            closeThought(at);
            const text = contentText(u.content);
            if (isRetryNotice(text)) {
              const l = last();
              const note = `Model request failed; ${text.trim().replace(/^Retrying/, "retrying").replace(/\.\.\.$/, "…")}`;
              if (l?.kind === "note" && l.text.startsWith("Model request failed")) l.text = note;
              else items.push({ kind: "note", text: note });
              break;
            }
            const l = last();
            if (l?.kind === "text") l.text += text;
            else if (text) items.push({ kind: "text", text });
            break;
          }
          case "agent_thought_chunk": {
            const text = contentText(u.content);
            const l = last();
            if (l?.kind === "thought" && l.ended == null) l.text += text;
            else if (text) items.push({ kind: "thought", text, started: at, ended: null });
            break;
          }
          case "tool_call": {
            closeThought(at);
            let t = tools.get(u.toolCallId);
            if (!t) {
              t = {
                kind: "tool",
                id: u.toolCallId,
                title: "",
                toolKind: "other",
                status: "pending",
                text: [],
                diffs: [],
                output: "",
                exitCode: null,
                rawInput: undefined,
                paths: [],
                started: at,
                ended: null,
              };
              tools.set(u.toolCallId, t);
              items.push(t);
            }
            applyToolFields(t, u, at);
            break;
          }
          case "tool_call_update": {
            const t = tools.get(u.toolCallId);
            if (t) applyToolFields(t, u, at);
            break;
          }
          case "plan": {
            const existing = items.findLast((i) => i.kind === "plan");
            if (existing && existing.kind === "plan" && existing === last()) existing.entries = u.entries ?? [];
            else items.push({ kind: "plan", entries: u.entries ?? [] });
            break;
          }
          case "config_option_update":
            if (Array.isArray(u.configOptions)) config = u.configOptions;
            break;
          case "user_message_chunk":
            // Echo of our own prompt from some adapters; we already have it.
            break;
        }
        break;
      }
    }
  }

  const waiting = items.some((i) => i.kind === "permission" && i.chosen === undefined);
  return { items, config, agentName, busy, waiting };
}
