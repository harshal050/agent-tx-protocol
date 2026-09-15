import nextVitals from "eslint-config-next/core-web-vitals";
import nextTypescript from "eslint-config-next/typescript";
import { defineConfig, globalIgnores } from "eslint/config";

/** Shared ESLint flat config for Next.js apps. */
export default defineConfig(
  nextVitals,
  nextTypescript,
  {
    rules: {
      "@typescript-eslint/consistent-type-imports": ["error", { fixStyle: "inline-type-imports" }],
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_", caughtErrorsIgnorePattern: "^_" },
      ],
      eqeqeq: ["error", "smart"],
    },
  },
  globalIgnores([".next/**", "out/**", "build/**", "next-env.d.ts"]),
);
