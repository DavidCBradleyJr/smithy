import { describe, expect, it } from "vitest";
import { reduce, type RawEvent } from "./reduce";

const upd = (update: any, at = 0): RawEvent => ({ t: "update", update, at });

// Shapes captured from pi-acp 0.0.33 driving Bonsai on 2026-09-21.
const config = [
  { type: "select", id: "model", category: "model", name: "Model", currentValue: "bonsai/bonsai-2-27b", options: [{ value: "bonsai/bonsai-2-27b", name: "bonsai/Bonsai 2 27B (local)" }] },
];
const turn: RawEvent[] = [
  { t: "session", resumed: false, agentInfo: { name: "pi-acp", title: "pi ACP adapter" }, configOptions: config, at: 1 },
  { t: "user", text: "add c", at: 1000 },
  upd({ sessionUpdate: "agent_thought_chunk", content: { type: "text", text: "Let me " } }, 1100),
  upd({ sessionUpdate: "agent_thought_chunk", content: { type: "text", text: "look." } }, 1200),
  upd({ sessionUpdate: "tool_call", toolCallId: "t1", title: "cat calc.py", kind: "execute", status: "pending", content: [{ type: "terminal", terminalId: "t1" }] }, 1300),
  upd({ sessionUpdate: "tool_call_update", toolCallId: "t1", status: "in_progress", _meta: { terminal_output: { data: "def add(a, b):\n" } } }, 1400),
  upd({ sessionUpdate: "tool_call_update", toolCallId: "t1", status: "in_progress", _meta: { terminal_output: { data: "    return a + b\n" } } }, 1450),
  upd({ sessionUpdate: "tool_call_update", toolCallId: "t1", status: "completed", _meta: { terminal_exit: { exit_code: 0, signal: null } } }, 1500),
  upd({ sessionUpdate: "tool_call", toolCallId: "t2", title: "edit", kind: "edit", status: "pending", locations: [{ path: "/p/calc.py" }] }, 1600),
  upd({ sessionUpdate: "tool_call_update", toolCallId: "t2", status: "completed", content: [{ type: "diff", path: "/p/calc.py", oldText: "a\n", newText: "b\n" }] }, 1700),
  upd({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Done" } }, 1800),
  upd({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: "." } }, 1810),
  { t: "stop", reason: "end_turn", at: 2000 },
];

describe("reduce", () => {
  it("folds a full pi-acp turn", () => {
    const v = reduce(turn);
    expect(v.agentName).toBe("pi ACP adapter");
    expect(v.config[0].currentValue).toBe("bonsai/bonsai-2-27b");
    expect(v.items.map((i) => i.kind)).toEqual(["user", "thought", "tool", "tool", "text", "stop"]);

    const [, thought, run, edit, text, stop] = v.items as any[];
    expect(thought.text).toBe("Let me look.");
    expect(thought.ended).toBe(1300);
    expect(run.output).toBe("def add(a, b):\n    return a + b\n");
    expect(run.exitCode).toBe(0);
    expect(run.status).toBe("completed");
    expect(run.ended - run.started).toBe(200);
    expect(edit.diffs).toEqual([{ path: "/p/calc.py", oldText: "a\n", newText: "b\n" }]);
    expect(edit.paths).toEqual(["/p/calc.py"]);
    expect(text.text).toBe("Done.");
    expect(stop.took).toBe(1000);
    expect(v.busy).toBe(false);
  });

  it("tracks pending and resolved permissions", () => {
    const base: RawEvent[] = [
      { t: "user", text: "rm it", at: 1 },
      { t: "permission", key: "th:1", toolCall: { title: "rm x" }, options: [{ optionId: "allow", name: "Allow", kind: "allow_once" }] },
    ];
    expect(reduce(base).waiting).toBe(true);
    const done = reduce([...base, { t: "permission_resolved", key: "th:1", optionId: "allow" }]);
    expect(done.waiting).toBe(false);
    expect((done.items[1] as any).chosen).toBe("allow");
  });

  it("marks a crashed turn and a fresh session after history", () => {
    const v = reduce([{ t: "user", text: "x", at: 1 }, { t: "exit" }, { t: "session", resumed: false, configOptions: [] }]);
    expect(v.items.map((i) => i.kind)).toEqual(["user", "error", "note"]);
  });

  it("config_option_update replaces config", () => {
    const v = reduce([upd({ sessionUpdate: "config_option_update", configOptions: [{ ...config[0], currentValue: "x" }] })]);
    expect(v.config[0].currentValue).toBe("x");
  });

  it("turns Pi's retry chatter into one note, not an answer", () => {
    const v = reduce([
      { t: "user", text: "who are you?", at: 1 },
      upd({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Retrying (attempt 1/3, waiting 2s)..." } }),
      upd({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Retrying (attempt 2/3, waiting 4s)..." } }),
      upd({ sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Retry finished, resuming." } }),
      { t: "error", message: "The local model rejected the request: Unexpected reasoning effort high." },
      { t: "stop", reason: "end_turn", at: 5 },
    ]);
    expect(v.items.map((i) => i.kind)).toEqual(["user", "note", "error", "stop"]);
    expect((v.items[1] as any).text).toBe("Model request failed; Retry finished, resuming.");
  });
});
