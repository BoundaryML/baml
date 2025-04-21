### BAML ESM Reproduction

1. `pnpm i`
2. `pnpm build`
3. `OPENAI_API_KEY=MY_KEY pnpm start`

_Expected Result_

Parsed resume is outputted.

_Actual Result_

```
node:internal/modules/esm/resolve:275
    throw new ERR_MODULE_NOT_FOUND(
          ^

Error [ERR_MODULE_NOT_FOUND]: Cannot find module '/baml-esm-repro/dist/baml_client/async_client' imported from /baml-esm-repro/dist/baml_client/index.js
    at finalizeResolution (node:internal/modules/esm/resolve:275:11)
    at moduleResolve (node:internal/modules/esm/resolve:860:10)
    at defaultResolve (node:internal/modules/esm/resolve:984:11)
    at ModuleLoader.defaultResolve (node:internal/modules/esm/loader:719:12)
    at #cachedDefaultResolve (node:internal/modules/esm/loader:643:25)
    at ModuleLoader.resolve (node:internal/modules/esm/loader:626:38)
    at ModuleLoader.getModuleJobForImport (node:internal/modules/esm/loader:279:38)
    at ModuleJob._link (node:internal/modules/esm/module_job:136:49) {
  code: 'ERR_MODULE_NOT_FOUND',
  url: 'file:///baml-esm-repro/dist/baml_client/async_client'
}
```