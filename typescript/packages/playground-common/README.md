The fiddle project uses the src/ directory directly to get the components.

Nextjs.config has a transpilePackages config that bundles this package in with the nextjs app.

Therefore there is no bundling necessary in this package (unless the VSCode playground web panel requires it, TBD)

See example: https://github.dev/vercel/examples/blob/952de642d3e74b02e1fda5db984446020d2cb81e/solutions/monorepo/packages/ui/tsconfig.json
