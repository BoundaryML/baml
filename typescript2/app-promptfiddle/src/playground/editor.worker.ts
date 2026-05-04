// Local worker entry for Monaco's editor worker.
// Webpack only bundles a worker (resolving its imports) when the URL appears
// inside `new Worker(new URL('./local-file', import.meta.url))`. Pointing that
// URL at a node_modules file causes webpack/Next.js to copy it as a raw asset,
// which fails because the upstream file is just `export * from '<bare specifier>'`.
import '@codingame/monaco-vscode-editor-api/esm/vs/editor/editor.worker.js';
