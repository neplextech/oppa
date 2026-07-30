import eslint from '@eslint/js';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import globals from 'globals';
import tseslint from 'typescript-eslint';

const typedProjects = [
  {
    files: ['apps/oppa/src/**/*.{ts,tsx}'],
    project: './apps/oppa/tsconfig.json',
  },
  {
    files: ['apps/oppa/vite.config.ts'],
    project: './apps/oppa/tsconfig.node.json',
  },
  {
    files: ['apps/www/**/*.{ts,tsx}'],
    project: './apps/www/tsconfig.json',
  },
  {
    files: ['packages/protocol/**/*.{ts,tsx}'],
    project: './packages/protocol/tsconfig.test.json',
  },
  {
    files: ['packages/server/**/*.{ts,tsx}'],
    project: './packages/server/tsconfig.test.json',
  },
  {
    files: ['examples/node-server/**/*.{ts,tsx}'],
    project: './examples/node-server/tsconfig.test.json',
  },
];

export default tseslint.config(
  {
    ignores: [
      '**/dist/**',
      '**/.next/**',
      '**/.fumadocs/**',
      '**/.source/**',
      '**/coverage/**',
      '**/out/**',
      '**/target/**',
      'protocol/schema/openprinter.schema.json',
    ],
  },
  eslint.configs.recommended,
  {
    files: ['**/*.{js,mjs,cjs}'],
    languageOptions: {
      globals: globals.node,
    },
  },
  ...tseslint.configs.recommendedTypeChecked.map((config) => ({
    ...config,
    files: ['**/*.{ts,tsx}'],
  })),
  {
    files: ['**/*.{ts,tsx}'],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
    rules: {
      '@typescript-eslint/consistent-type-imports': 'error',
      '@typescript-eslint/no-floating-promises': 'error',
      '@typescript-eslint/no-misused-promises': 'error',
    },
  },
  ...typedProjects.map(({ files, project }) => ({
    files,
    languageOptions: {
      parserOptions: {
        project,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  })),
  {
    files: ['apps/{oppa,www}/**/*.{ts,tsx}'],
    plugins: {
      'react-hooks': reactHooks,
    },
    rules: reactHooks.configs.recommended.rules,
  },
  {
    files: ['apps/oppa/src/**/*.tsx'],
    plugins: {
      'react-refresh': reactRefresh,
    },
    rules: {
      'react-refresh/only-export-components': ['warn', { allowConstantExport: true }],
    },
  },
);
