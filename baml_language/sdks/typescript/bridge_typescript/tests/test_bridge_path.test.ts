// test_bridge_path.test.ts — BAML_BRIDGE_PATH validation (no native library needed).

import { describe, expect, it } from 'vitest';
import { applyBridgePathOverride } from '../dist/bridge_path.js';

describe('BAML_BRIDGE_PATH', () => {
  it('is a no-op when unset or empty', () => {
    const unset: NodeJS.ProcessEnv = {};
    applyBridgePathOverride(unset);
    expect(unset.NAPI_RS_NATIVE_LIBRARY_PATH).toBeUndefined();

    const empty: NodeJS.ProcessEnv = { BAML_BRIDGE_PATH: '' };
    applyBridgePathOverride(empty);
    expect(empty.NAPI_RS_NATIVE_LIBRARY_PATH).toBeUndefined();
  });

  it('rejects a relative path', () => {
    expect(() => applyBridgePathOverride({ BAML_BRIDGE_PATH: 'baml_node.node' })).toThrow(/absolute/);
  });

  it('rejects a missing file', () => {
    expect(() => applyBridgePathOverride({ BAML_BRIDGE_PATH: '/nonexistent/baml_node.node' })).toThrow(
      /does not exist/,
    );
  });
});
