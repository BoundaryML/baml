The fiddle project uses the src/ directory directly to get the components.

Nextjs.config has a transpilePackages config that bundles this package in with the nextjs app.

Therefore there is no bundling necessary in this package (unless the VSCode playground web panel requires it, TBD)
