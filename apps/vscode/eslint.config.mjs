import base from "@agenttx/eslint-config/base";
import { defineConfig } from "eslint/config";
import globals from "globals";

export default defineConfig(base, {
  // The setup panel script runs inside a VS Code webview (a browser page).
  files: ["media/**/*.js"],
  languageOptions: {
    sourceType: "script",
    globals: { ...globals.browser, acquireVsCodeApi: "readonly" },
  },
});
