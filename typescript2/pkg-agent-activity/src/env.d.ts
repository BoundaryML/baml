// The subset of Vite's `import.meta.env` this package reads. The package is
// compiled by the host app's Vite build, so the values are always present.
interface ImportMetaEnv {
  readonly DEV: boolean;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
