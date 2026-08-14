/**
 * Host-app configuration for the playground proxy env var.
 *
 * The WASM runtime reads `BOUNDARY_PROXY_URL` to route LLM requests through a
 * proxy that injects provider API keys (see sys_llm/build_request). Whether that
 * var is surfaced in the env-vars dialog differs by deployment:
 *
 *   - app-promptfiddle always routes through its proxy and shows a dedicated
 *     on/off toggle for it (defaulting on).
 *   - The VS Code extension and the baml-cli playground hide it (users bring
 *     their own keys), even if it happens to be present in the shell env.
 *
 * The hosting app configures this once at startup; ExecutionPanel reads it and
 * passes it into ApiKeysDialog as plain props.
 */

/** Env var the WASM runtime reads to route requests through the playground proxy. */
export const BOUNDARY_PROXY_URL_KEY = 'BOUNDARY_PROXY_URL';

export interface ProxyEnvVarConfig {
  /** Whether to surface the proxy toggle in the env-vars dialog. */
  visible: boolean;
  /** Canonical proxy URL shown in the dialog and written to the BAML env when on. */
  url: string;
}

let config: ProxyEnvVarConfig = { visible: false, url: '' };

/** Configure how `BOUNDARY_PROXY_URL` is surfaced. Hidden by default. */
export function configureProxyEnvVar(next: Partial<ProxyEnvVarConfig>): void {
  config = { ...config, ...next };
}

/** Current proxy env var configuration. */
export function getProxyEnvVarConfig(): ProxyEnvVarConfig {
  return config;
}
