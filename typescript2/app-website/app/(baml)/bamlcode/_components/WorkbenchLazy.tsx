'use client';

import dynamic from 'next/dynamic';
import type { Problem } from '../_lib/types';

/**
 * Client-only boundary for the grading workbench. Monaco + the BexVM worker
 * (which imports `@b/bridge_wasm`) are browser-only and cannot be resolved
 * during SSR, so we load with `ssr: false` - mirrors learn2's `BamlEditorLazy`.
 */
const Workbench = dynamic(
  () => import('./Workbench').then((m) => m.Workbench),
  {
    loading: () => (
      <section className="bc-workspace">
        <div className="bc-editor bc-editor-loading font-mono">
          loading editor…
        </div>
      </section>
    ),
    ssr: false,
  },
);

export default function WorkbenchLazy({ problem }: { problem: Problem }) {
  return <Workbench problem={problem} />;
}
