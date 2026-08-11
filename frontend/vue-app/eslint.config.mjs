import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import pluginVue from 'eslint-plugin-vue';
import globals from 'globals';

export default [
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...pluginVue.configs['flat/recommended'],
  {
    languageOptions: {
      // Make browser globals (window, document, File, Event, ...) available
      // so DOM references in .vue/.ts files don't trigger no-undef.
      globals: {
        ...globals.browser,
      },
    },
  },
  {
    files: ['**/*.{ts,tsx,vue}'],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
        // Standard DOM Fetch types are type-only (from lib.dom.d.ts) and not
        // part of the runtime `globals` package, so declare them explicitly.
        RequestInit: 'readonly',
      },
    },
  },
  {
    files: ['**/*.vue'],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
      },
    },
  },
  {
    rules: {
      // TypeScript
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'warn',
      '@typescript-eslint/explicit-function-return-type': 'off',
      '@typescript-eslint/no-non-null-assertion': 'warn',

      // Vue
      'vue/multi-word-component-names': 'off',
      'vue/no-v-html': 'warn',
      'vue/require-default-prop': 'off',
      'vue/max-attributes-per-line': ['warn', { singleline: 3 }],

      // General
      'no-console': ['warn', { allow: ['warn', 'error'] }],
      'no-debugger': 'warn',
      'prefer-const': 'warn',
      'no-var': 'error',
      // TypeScript already reports genuine references to undefined variables;
      // `no-undef` is unreliable on .ts/.vue (false positives on types, DOM
      // interfaces, and interface members) and is disabled by
      // typescript-eslint's recommended config. Disable it deterministically.
      'no-undef': 'off',
    },
  },
  {
    ignores: ['dist/', 'node_modules/', '**/*.d.ts'],
  },
];
