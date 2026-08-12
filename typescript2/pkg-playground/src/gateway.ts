/**
 * Boundary LLM gateway (proxy) toggle + playground env defaults.
 *
 * The gateway is a single on/off toggle (see ApiKeysDialog) backed by the
 * `BOUNDARY_PROXY_URL` env var the WASM runtime reads to route requests through
 * our proxy. The behavior is deliberately unmagical:
 *   - toggle ON  => `BOUNDARY_PROXY_URL` is set to the proxy URL;
 *   - toggle OFF => `BOUNDARY_PROXY_URL` is removed.
 *
 * On first load we seed placeholder provider keys and turn the gateway on, so a
 * fresh playground can call OpenAI/Anthropic through the proxy with no setup.
 * The proxy injects the real keys server-side; the placeholders just keep the
 * runtime from suspending on a missing key. (The worker treats
 * `BOUNDARY_PROXY_URL` as optional so toggling the gateway off never prompts.)
 */

import { defaultSessionStore, type SessionStore } from './session-store';
import { BOUNDARY_PROXY_URL_KEY, getProxyEnvVarConfig } from './proxy-config';

/** Provider keys seeded on first load; the proxy swaps in real keys upstream.
 * Every key here must have a matching entry in the proxy allowlist
 * (app-fiddleproxy/server.js) so the placeholder is replaced server-side. */
const DEFAULT_ENV_VARS: Record<string, string> = {
  OPENAI_API_KEY: 'placeholder',
  ANTHROPIC_API_KEY: 'placeholder',
  AI_GATEWAY_API_KEY: 'placeholder',
};
// Bump the version suffix whenever DEFAULT_ENV_VARS gains a key, so existing
// visitors re-seed once and pick up the new placeholder (v2 added
// AI_GATEWAY_API_KEY). User-entered values still win — the seed merges them last.
const DEFAULTS_SEEDED_STORAGE_KEY = 'baml-playground-defaults-seeded-v2';

let store: SessionStore = defaultSessionStore;

function defaultsAlreadySeeded(): boolean {
  if (typeof window === 'undefined') return true;
  try {
    return window.localStorage.getItem(DEFAULTS_SEEDED_STORAGE_KEY) === 'true';
  } catch {
    return false;
  }
}

function markDefaultsSeeded(): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(DEFAULTS_SEEDED_STORAGE_KEY, 'true');
  } catch {
    /* localStorage unavailable */
  }
}

/**
 * Seed playground env defaults exactly once: placeholder provider keys and the
 * gateway turned on. Existing/persisted values always win, so this never
 * clobbers anything the user set. Call once at startup.
 */
export function initPlaygroundEnv(s: SessionStore = defaultSessionStore): void {
  store = s;
  if (defaultsAlreadySeeded()) return;
  const proxyUrl = getProxyEnvVarConfig().url;
  s.setEnvVars((prev) => ({
    ...DEFAULT_ENV_VARS,
    ...(proxyUrl ? { [BOUNDARY_PROXY_URL_KEY]: proxyUrl } : {}),
    ...prev,
  }));
  markDefaultsSeeded();
}

/** Toggle the gateway: set or remove `BOUNDARY_PROXY_URL`. */
export function setGatewayEnabled(enabled: boolean): void {
  if (enabled) {
    const proxyUrl = getProxyEnvVarConfig().url;
    store.setEnvVars((prev) => ({ ...prev, [BOUNDARY_PROXY_URL_KEY]: proxyUrl }));
  } else {
    store.setEnvVars((prev) => {
      const { [BOUNDARY_PROXY_URL_KEY]: _omit, ...rest } = prev;
      return rest;
    });
  }
}
