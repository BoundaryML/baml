'use client';

import type { ReactNode } from 'react';
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels';
import { BamlReference } from './BamlReference';

/**
 * Resizable statement | workspace | syntax layout for the solve page. Sizes
 * persist in localStorage via `autoSaveId`. The vertical editor | console split
 * lives inside the workbench.
 */
export function SolveLayout({
  statement,
  workbench,
}: {
  statement: ReactNode;
  workbench: ReactNode;
}) {
  return (
    <PanelGroup
      direction="horizontal"
      autoSaveId="bamlcode-body-3"
      className="bc-solve-body"
    >
      <Panel defaultSize={30} minSize={16}>
        <section className="bc-statement">{statement}</section>
      </Panel>
      <PanelResizeHandle className="bc-handle bc-handle-v" />
      <Panel defaultSize={48} minSize={28}>
        {workbench}
      </Panel>
      <PanelResizeHandle className="bc-handle bc-handle-v" />
      <Panel defaultSize={22} minSize={0} collapsible>
        <BamlReference />
      </Panel>
    </PanelGroup>
  );
}
