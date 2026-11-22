import { fileURLToPath } from "node:url";
import js from "@eslint/js";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";

export default tseslint.config(
  { ignores: ["node_modules/**", "dist/**", "coverage/**", "ci-artifacts/**", "src-tauri/**"] },
  {
    files: [
      "scripts/quality/**/*.mjs",
      "scripts/ci/**/*.mjs",
      "scripts/e2e/**/*.mjs",
      "tests/frontend/e2e/**/*.mjs",
      "tests/quality/**/*.mjs",
      "eslint.config.mjs",
    ],
    extends: [js.configs.recommended],
    languageOptions: {
      globals: {
        process: "readonly",
        console: "readonly",
        URL: "readonly",
        structuredClone: "readonly",
      },
    },
  },
  {
    files: ["tests/frontend/e2e/**/*.mjs"],
    languageOptions: {
      globals: {
        document: "readonly",
        localStorage: "readonly",
        sessionStorage: "readonly",
        describe: "readonly",
        it: "readonly",
        before: "readonly",
        after: "readonly",
        beforeEach: "readonly",
        afterEach: "readonly",
      },
    },
  },
  {
    files: ["src/**/*.{ts,tsx}", "tests/frontend/**/*.{ts,tsx}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: {
      parserOptions: {
        project: "./tsconfig.test.json",
        tsconfigRootDir: fileURLToPath(new URL(".", import.meta.url)),
      },
    },
    linterOptions: { reportUnusedDisableDirectives: "error" },
    rules: {
      "@typescript-eslint/no-floating-promises": "error",
      "@typescript-eslint/no-misused-promises": "error",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      "@typescript-eslint/ban-ts-comment": [
        "error",
        { "ts-expect-error": "allow-with-description" },
      ],
    },
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    plugins: { "react-hooks": reactHooks },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
    },
  },
);
