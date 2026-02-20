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
/** Latest port (updated on every setRuntimePort); used when reconnecting after restart. */
let currentPort: RuntimePort | null = null;
/** Connection version passed from MonacoEditor (survives HMR); used as React key to force remount. */
let currentConnectionVersion = 0;
/** Incremented when port changes on restart so ExecutionPanel remounts and requests state. */
let portKey = 0;
const portChangeListeners = new Set<(port: RuntimePort) => void>();

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

      const renderWithPort = (port: RuntimePort) => {
        import('@b/pkg-playground').then(({ ExecutionPanel }) => {
          this.reactRoot?.render(
            createElement(ExecutionPanel, {
              port,
              key: `playground-${currentConnectionVersion}`,
              connectionVersion: currentConnectionVersion,
            }),
          );
        });
      };

      portPromise.then(renderWithPort);

      const onPortChange = (port: RuntimePort) => {
        renderWithPort(port);
      };
      portChangeListeners.add(onPortChange);
      const removeListener = () => portChangeListeners.delete(onPortChange);
      (this as any)._portChangeCleanup = removeListener;

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
      const cleanup = (this as any)._portChangeCleanup as (() => void) | undefined;
      if (typeof cleanup === 'function') cleanup();
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

  vscode.commands.registerCommand('baml.openPlayground', () => {
    openPlaygroundTab();
  });
}

// ---------------------------------------------------------------------------
// Public API — called from MonacoEditor.tsx
// ---------------------------------------------------------------------------

export interface SetRuntimePortOptions {
  /** Connection version (0, 1, 2, ...) used as React key to force ExecutionPanel remount on restart. */
  connectionVersion?: number;
}

/**
 * Provide the RuntimePort and open the Playground tab.
 * On first call: resolves the port promise and opens the tab.
 * On restart: updates the existing pane with the new port (no new tab).
 */
export function setRuntimePort(port: RuntimePort, options?: SetRuntimePortOptions): void {
  currentPort = port;
  currentConnectionVersion = options?.connectionVersion ?? currentConnectionVersion;
  if (portResolve) {
    portResolve(port);
    portResolve = null;
    openPlaygroundTab();
  } else {
    portKey += 1;
    portChangeListeners.forEach((cb) => cb(port));
  }
}
