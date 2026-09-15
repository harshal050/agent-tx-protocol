import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // The first Shiki highlight loads grammars and the regex engine (several seconds cold).
    testTimeout: 30_000,
  },
});
