/**
 * Custom result renderers for Playground output.
 *
 * Results that include a `$baml: { type: "..." }` discriminator can be
 * rendered with a registered React component (e.g. curl for baml.http.Request).
 */

import type { FC } from 'react';

export const BAML_TYPE_KEY = '$baml' as const;
export const BAML_TYPE_FIELD = 'type' as const;

/** Props passed to a custom result renderer. */
export interface ResultRendererProps {
  /** The parsed result value (object with $baml.type when from BAML). */
  value: unknown;
}

/** Extract BAML type from a result value, e.g. "baml.http.Request". */
export function getBamlType(value: unknown): string | null {
  if (value == null || typeof value !== 'object') return null;
  const baml = (value as Record<string, unknown>)[BAML_TYPE_KEY];
  if (baml == null || typeof baml !== 'object') return null;
  const type = (baml as Record<string, unknown>)[BAML_TYPE_FIELD];
  return typeof type === 'string' ? type : null;
}

/** Registry: BAML type string -> React component. */
const registry = new Map<string, FC<ResultRendererProps>>();

/**
 * Register a React component to render results of a given BAML type.
 * Example: registerResultRenderer('baml.http.Request', HttpRequestCurlRenderer);
 */
export function registerResultRenderer(type: string, Component: FC<ResultRendererProps>): void {
  registry.set(type, Component);
}

/**
 * Get the renderer component for a BAML type, or undefined if none registered.
 */
export function getResultRenderer(type: string): FC<ResultRendererProps> | undefined {
  return registry.get(type);
}

/**
 * Return all currently registered (type, Component) pairs.
 * Used by ResultDisplay to resolve renderers.
 */
export function getRegisteredResultRenderers(): Map<string, FC<ResultRendererProps>> {
  return new Map(registry);
}
