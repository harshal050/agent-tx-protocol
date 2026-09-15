import reactHooks from "eslint-plugin-react-hooks";
import { defineConfig } from "eslint/config";
import globals from "globals";

import base from "./base.js";

/** Shared ESLint flat config for React libraries. */
export default defineConfig(base, reactHooks.configs.flat["recommended-latest"], {
  languageOptions: {
    globals: { ...globals.browser },
  },
});
