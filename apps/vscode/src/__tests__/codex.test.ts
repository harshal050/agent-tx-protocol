import { parse } from "smol-toml";
import { describe, expect, it } from "vitest";

import { codexFormat } from "../config/codex";
import { ConfigError } from "../config/common";

const entry = {
  command: "/home/you/.agenttx/bin/agenttx",
  args: ["mcp", "--home", "/home/you/.agenttx/clients/codex"],
};

describe("Codex config.toml", () => {
  it("adds AgentTx to a new file", () => {
    const text = codexFormat.upsert(undefined, entry);
    expect(parse(text)).toEqual({ mcp_servers: { agenttx: entry } });
    expect(codexFormat.read(text)).toEqual({ kind: "present", entry, enabled: true });
  });

  it("replaces an old entry and keeps everything else", () => {
    const original = [
      'model = "gpt-5"',
      "",
      "# my servers",
      "[mcp_servers.docs]",
      'command = "docs-mcp"',
      "",
      "[mcp_servers.agenttx]",
      'command = "agenttx"',
      'args = ["mcp"]',
      "enabled = false",
      "",
      "[mcp_servers.agenttx.env]",
      'TOKEN = "x"',
      "",
      '[projects."/home/you"]',
      'trust_level = "trusted"',
      "",
    ].join("\n");

    const text = codexFormat.upsert(original, entry);
    const doc = parse(text);
    expect(doc.model).toBe("gpt-5");
    expect(doc.mcp_servers).toEqual({ docs: { command: "docs-mcp" }, agenttx: entry });
    expect(doc.projects).toEqual({ "/home/you": { trust_level: "trusted" } });
    expect(text).toContain("# my servers");
  });

  it("recognises a quoted table name", () => {
    const text = codexFormat.upsert('[mcp_servers."agenttx"]\ncommand = "agenttx"\n', entry);
    expect(parse(text)).toEqual({ mcp_servers: { agenttx: entry } });
  });

  it("writes Windows paths with backslashes and spaces", () => {
    const windows = { command: "C:\\Users\\Ann Lee\\.agenttx\\bin\\agenttx.exe", args: ["mcp"] };
    const text = codexFormat.upsert(undefined, windows);
    expect(codexFormat.read(text)).toEqual({ kind: "present", entry: windows, enabled: true });
  });

  it("reports an entry that is turned off", () => {
    const text = '[mcp_servers.agenttx]\ncommand = "agenttx"\nenabled = false\n';
    expect(codexFormat.read(text)).toMatchObject({ kind: "present", enabled: false });
  });

  it("removes only AgentTx", () => {
    const text = codexFormat.upsert('[mcp_servers.docs]\ncommand = "docs-mcp"\n', entry);
    const removed = codexFormat.remove(text);
    expect(parse(removed)).toEqual({ mcp_servers: { docs: { command: "docs-mcp" } } });
    expect(removed).not.toContain("AgentTx");
    expect(codexFormat.remove(removed)).toBe(removed);
  });

  it("never edits a file it can't read", () => {
    expect(codexFormat.read("[oops")).toMatchObject({ kind: "invalid" });
    expect(() => codexFormat.upsert("[oops", entry)).toThrow(ConfigError);
  });

  it("refuses an inline entry it can't rewrite safely", () => {
    const inline = '[mcp_servers]\nagenttx = { command = "agenttx" }\n';
    expect(() => codexFormat.upsert(inline, entry)).toThrow(ConfigError);
  });
});
