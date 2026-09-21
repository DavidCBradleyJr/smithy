// Review comments left on diffs, and the prompt that hands them to the agent.

export interface ReviewComment {
  path: string;
  /** Line number in the new file, or the old file for a removed line. */
  line: number;
  side: "new" | "old";
  code: string;
  text: string;
}

/** The prompt that hands review comments back to the agent. */
export function reviewPrompt(comments: ReviewComment[]) {
  const byFile = new Map<string, ReviewComment[]>();
  for (const c of comments) byFile.set(c.path, [...(byFile.get(c.path) ?? []), c]);
  const parts = ["I reviewed your changes. Please address these comments:"];
  for (const [path, cs] of byFile) {
    parts.push("", `${path}`);
    for (const c of cs.sort((a, b) => a.line - b.line)) {
      const where = c.side === "old" ? `removed line ${c.line}` : `line ${c.line}`;
      parts.push(`- ${where}: \`${c.code.trim()}\``, `  ${c.text.trim().replaceAll("\n", "\n  ")}`);
    }
  }
  return parts.join("\n");
}
