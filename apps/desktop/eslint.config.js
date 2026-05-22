// ESLint 9 flat config.
import js from "@eslint/js";
import tsparser from "@typescript-eslint/parser";
import tsplugin from "@typescript-eslint/eslint-plugin";
import react from "eslint-plugin-react";
import reactHooks from "eslint-plugin-react-hooks";

export default [
  {
    ignores: [
      "dist/**",
      "node_modules/**",
      "src-tauri/**",
      "vite.config.ts",
      "tailwind.config.ts",
      "postcss.config.js",
      "eslint.config.js",
    ],
  },
  js.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsparser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: "module",
        ecmaFeatures: { jsx: true },
        project: "./tsconfig.json",
      },
      globals: {
        window: "readonly",
        document: "readonly",
        navigator: "readonly",
        console: "readonly",
        HTMLElement: "readonly",
        HTMLInputElement: "readonly",
        HTMLButtonElement: "readonly",
        HTMLTextAreaElement: "readonly",
        KeyboardEvent: "readonly",
        Element: "readonly",
        JSX: "readonly",
        React: "readonly",
        setTimeout: "readonly",
        clearTimeout: "readonly",
        setInterval: "readonly",
        clearInterval: "readonly",
        queueMicrotask: "readonly",
      },
    },
    plugins: {
      "@typescript-eslint": tsplugin,
      react,
      "react-hooks": reactHooks,
    },
    settings: { react: { version: "detect" } },
    rules: {
      ...tsplugin.configs.recommended.rules,
      ...react.configs.recommended.rules,
      ...react.configs["jsx-runtime"].rules,
      ...reactHooks.configs.recommended.rules,
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      "@typescript-eslint/consistent-type-imports": "error",
      "react/prop-types": "off",
      // The default `react/no-unescaped-entities` rule fires on every
      // legitimate apostrophe and quote in copy. Our UI strings are
      // English prose; we'd rather write "user's" than "user&apos;s".
      "react/no-unescaped-entities": "off",
    },
  },
];
