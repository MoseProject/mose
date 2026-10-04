import js from "@eslint/js";
import prettier from "eslint-config-prettier";
import react from "eslint-plugin-react";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "coverage", "node_modules", "src-tauri"] },
  { settings: { react: { version: "detect" } } },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  react.configs.flat.recommended,
  react.configs.flat["jsx-runtime"],
  reactHooks.configs.flat.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2023,
      globals: { ...globals.browser },
    },
    rules: {
      "react/prop-types": "off",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  {
    files: ["**/*.test.{ts,tsx}", "src/testSupport.ts"],
    languageOptions: { globals: { ...globals.node } },
  },
  // Prettier owns formatting; the rules below keep every statement and arrow body on its own
  // lines, which Prettier alone would leave inline.
  prettier,
  {
    files: ["**/*.{ts,tsx}"],
    rules: {
      curly: ["error", "all"],
      "arrow-body-style": ["error", "always"],
      "max-statements-per-line": ["error", { max: 1 }],
      "no-multi-assign": "error",
    },
  },
);
