import { describe, expect, it } from "vitest";

import { claudeFormat } from "../config/claude";
import { ConfigError } from "../config/common";

const entry = {
  command: "/home/you/.agenttx/bin/agenttx",
  args: ["mcp", "--home", "/home/you/.agenttx/clients/claude-code"],
};

describe("Claude Code .claude.json", () => {
  it("adds AgentTx the way `claude mcp add --scope user` does", () => {
    const text = claudeFormat.upsert(undefined, entry);
    expect(JSON.parse(text)).toEqual({
      mcpServers: { agenttx: { type: "stdio", ...entry, env: {} } },
    });
    expect(text.endsWith("\n")).toBe(true);
    expect(claudeFormat.read(text)).toEqual({ kind: "present", entry, enabled: true });
  });

  it("keeps other settings, their order and the indentation", () => {
    const original = JSON.stringify(
      {
        numStartups: 3,
        mcpServers: { docs: { type: "stdio", command: "docs" } },
        projects: { "/x": {} },
      },
      null,
      2,
    );
    const text = claudeFormat.upsert(original, entry);
    const doc = JSON.parse(text);
    expect(Object.keys(doc)).toEqual(["numStartups", "mcpServers", "projects"]);
    expect(Object.keys(doc.mcpServers)).toEqual(["docs", "agenttx"]);
    expect(text.startsWith('{\n  "numStartups"')).toBe(true);
    expect(text.endsWith("\n")).toBe(false);
  });

  it("removes only AgentTx", () => {
    const text = claudeFormat.upsert(
      '{\n  "mcpServers": { "docs": { "command": "docs" } }\n}\n',
      entry,
    );
    const removed = claudeFormat.remove(text);
    expect(JSON.parse(removed)).toEqual({ mcpServers: { docs: { command: "docs" } } });
    expect(claudeFormat.remove(removed)).toBe(removed);
  });

  it("never edits a file it can't read", () => {
    expect(claudeFormat.read("{ nope")).toMatchObject({ kind: "invalid" });
    expect(() => claudeFormat.upsert("{ nope", entry)).toThrow(ConfigError);
    expect(() => claudeFormat.upsert("[]", entry)).toThrow(ConfigError);
  });
});
