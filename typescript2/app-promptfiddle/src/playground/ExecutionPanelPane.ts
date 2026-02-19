/**
 * ExecutionPanelPane — registers a custom Monaco EditorPane that hosts
 * the ExecutionPanel React component inside the VS Code workbench.
 *
 * The pane appears as an editor tab ("Playground") beside .baml file tabs.
 *
 * Usage (in MonacoEditor.tsx after apiWrapper.start()):
 *   1. Call registerExecutionPanelPane() once
 *   2. Call setRuntimePort(port) when the worker is ready
 *   3. The command "baml.openPlayground" or setRuntimePort both open the tab
 */

import { createRoot, type Root } from 'react-dom/client';
import { createElement } from 'react';
import type { RuntimePort } from '@b/pkg-playground';
import type { Dimension } from '@codingame/monaco-vscode-api/vscode/vs/base/browser/dom';

// ---------------------------------------------------------------------------
// Module-level state — bridges imperative EditorPane with React component
// ---------------------------------------------------------------------------

let portResolve: ((port: RuntimePort) => void) | null = null;
let portPromise = new Promise<RuntimePort>((resolve) => {
  portResolve = resolve;
});

const PANE_TYPE_ID = 'baml.executionPanel';

// Reference to the PlaygroundInput constructor (set during registration)
let PlaygroundInputCtor: (new () => any) | null = null;
let singletonInput: any = null;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Open (or reveal) the Playground editor tab beside the current editor. */
async function openPlaygroundTab(): Promise<void> {
  if (!PlaygroundInputCtor) return;
  if (!singletonInput || singletonInput.isDisposed?.()) {
    singletonInput = new PlaygroundInputCtor();
  }

  const { StandaloneServices } = await import('@codingame/monaco-vscode-api');
  const { IEditorService } = await import(
    '@codingame/monaco-vscode-api/vscode/vs/workbench/services/editor/common/editorService.service'
  );

  const editorService = StandaloneServices.get(IEditorService);
  const SIDE_GROUP = -2;
  await editorService.openEditor(singletonInput, { revealIfOpened: true }, SIDE_GROUP);
}

// ---------------------------------------------------------------------------
// Registration — call once after apiWrapper.start()
// ---------------------------------------------------------------------------

let registered = false;

export async function registerExecutionPanelPane(): Promise<void> {
  if (registered) return;
  registered = true;

  const [
    { SimpleEditorPane, SimpleEditorInput, registerEditorPane, EditorInputCapabilities },
    vscode,
  ] = await Promise.all([
    import('@codingame/monaco-vscode-api/service-override/tools/views'),
    import('vscode'),
  ]);

  // ── EditorInput: the "document" model for this tab ──────────────────

  class PlaygroundInput extends SimpleEditorInput {
    constructor() {
      super(undefined); // virtual editor, no file URI
      this.setName('Playground');
      this.setTitle('Playground');
      this.addCapability(EditorInputCapabilities.Singleton);
      this.addCapability(EditorInputCapabilities.Readonly);
    }

    get typeId(): string {
      return PANE_TYPE_ID;
    }

    get editorId(): string {
      return PANE_TYPE_ID;
    }
  }

  PlaygroundInputCtor = PlaygroundInput;

  // ── EditorPane: the visual container ────────────────────────────────

  class PlaygroundEditorPane extends SimpleEditorPane {
    private reactRoot: Root | null = null;
    private _el: HTMLElement | null = null;

    async renderInput() { return { dispose() {} }; }

    initialize(): HTMLElement {
      const el = document.createElement('div');
      el.style.overflow = 'hidden';
      el.style.display = 'flex';
      el.style.flexDirection = 'column';
      el.className = 'font-vsc text-vsc-text bg-vsc-panel';
      this._el = el;

      this.reactRoot = createRoot(el);

      // Show loading state, then swap to ExecutionPanel when port is ready
      this.reactRoot.render(
        createElement('div', {
          style: { padding: 20, color: '#888', fontSize: 12 },
        }, 'Loading playground…'),
      );

      portPromise.then(async (port) => {
        const { ExecutionPanel } = await import('@b/pkg-playground');
        this.reactRoot?.render(
          createElement(ExecutionPanel, { port }),
        );
      });

      return el;
    }

    // Explicitly size our container — the base class only sizes the
    // wrapper, so our React content needs actual pixel dimensions.
    layout(dimension: Dimension): void {
      super.layout(dimension);
      if (this._el) {
        this._el.style.width = `${dimension.width}px`;
        this._el.style.height = `${dimension.height}px`;
      }
    }

    dispose(): void {
      this.reactRoot?.unmount();
      this.reactRoot = null;
      this._el = null;
      super.dispose();
    }
  }

  registerEditorPane(
    PANE_TYPE_ID,
    'Playground',
    PlaygroundEditorPane as any,
    [PlaygroundInput],
  );

  // Register the command palette entry
  vscode.commands.registerCommand('baml.openPlayground', () => {
    openPlaygroundTab();
  });
}

// ---------------------------------------------------------------------------
// Public API — called from MonacoEditor.tsx
// ---------------------------------------------------------------------------

/**
 * Provide the RuntimePort and open the Playground tab.
 * Call this once when the WASM worker sends 'ready'.
 */
export function setRuntimePort(port: RuntimePort): void {
  if (portResolve) {
    portResolve(port);
    portResolve = null;
  }
  // Auto-open the tab when the runtime becomes available
  openPlaygroundTab();
}
