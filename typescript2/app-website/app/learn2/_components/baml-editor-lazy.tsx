'use client';

import dynamic from 'next/dynamic';

/**
 * Client-only boundary for the single-pane live BAML editor used inline in the
 * deck. Monaco + the BexVM worker are browser-only, so we load with `ssr: false`
 * (mirrors `LivePlaygroundLazy`). The editor brings real diagnostics, hover,
 * completion, and the test "▶ Run" codelenses from the shared runtime — the
 * "Open Playground" lens is filtered out in `BamlEditor`.
 */
const BamlEditor = dynamic(
  () => import('../_editor/baml-editor').then((m) => m.BamlEditor),
  {
    loading: () => (
      <div className="l2-bamled-wrap">
        <div className="l2-bamled-frame">
          <div className="l2-bamled l2-bamled-loading">loading editor…</div>
        </div>
      </div>
    ),
    ssr: false,
  },
);

export default BamlEditor;
