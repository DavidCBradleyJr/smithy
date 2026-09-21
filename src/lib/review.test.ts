import { describe, expect, it } from "vitest";
import { reviewPrompt } from "./review";

describe("reviewPrompt", () => {
  it("groups by file, orders by line, and quotes the code", () => {
    const p = reviewPrompt([
      { path: "src/b.rs", line: 9, side: "new", code: "  let x = 1;", text: "rename x" },
      { path: "src/a.rs", line: 12, side: "new", code: "foo()", text: "handle the error\nhere too" },
      { path: "src/a.rs", line: 3, side: "old", code: "bar()", text: "why remove this?" },
    ]);
    expect(p).toBe(
      [
        "I reviewed your changes. Please address these comments:",
        "",
        "src/b.rs",
        "- line 9: `let x = 1;`",
        "  rename x",
        "",
        "src/a.rs",
        "- removed line 3: `bar()`",
        "  why remove this?",
        "- line 12: `foo()`",
        "  handle the error",
        "  here too",
      ].join("\n"),
    );
  });
});
