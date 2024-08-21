module.exports = {
  root: true,
  parser: '@typescript-eslint/parser',
  plugins: ['@typescript-eslint'],
  env: {
    node: true,
    es6: true,
  },
  // This loads eslint-config-prettier, which disables formatting rules
  extends: ['prettier'],
  parserOptions: {
    ecmaVersion: 2020,
    sourceType: 'module',
  },
  overrides: [
    {
      files: ['*.ts', '*.tsx'],
      parserOptions: {
        project: ['./tsconfig.json', './packages/*/tsconfig.json'],
      },
      extends: [
        'plugin:react/recommended',
        'plugin:react-hooks/recommended',
        'plugin:@typescript-eslint/recommended',
        'plugin:@typescript-eslint/eslint-recommended',
        'plugin:@typescript-eslint/recommended-requiring-type-checking',
      ],
      rules: {
        '@typescript-eslint/no-floating-promises': 'off',
        '@typescript-eslint/no-misused-promises': 'warn',
        // no unused vars
        '@typescript-eslint/no-unused-vars': 'warn',
        // no require imports
        '@typescript-eslint/no-var-requires': 'warn',
        '@typescript-eslint/no-require-imports': 'warn',
      },
    },
    {
      files: ['./app/[project_id]/_playground_controller/TestState.ts'],
      rules: {
        '@typescript-eslint/no-unsafe-argument': 'warn',
      },
    },
    {
      files: ['app/[project_id]/_components/Tour.tsx'],
      rules: {
        'react/no-unescaped-entities': 'warn',
      },
    },
    {
      files: ['lib/atomWithHashBase64.ts'],
      rules: {
        '@typescript-eslint/no-unsafe-return': 'warn',
      },
    },
  ],
  rules: {
    '@typescript-eslint/no-unused-vars': 'off',
    'react/react-in-jsx-scope': 'off',
  },
}
