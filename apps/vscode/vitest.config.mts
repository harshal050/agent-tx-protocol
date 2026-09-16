import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    // The server check starts the real agenttx program.
    testTimeout: 30_000,
  },
});
