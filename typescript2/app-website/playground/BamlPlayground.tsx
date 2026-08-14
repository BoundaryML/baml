'use client';

import {
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';
import {
  ExecutionPanel,
  WorkerRuntimePort,
  type RuntimePort,
} from '@b/pkg-playground';
import { ChevronLeft, ChevronRight } from 'lucide-react';
import { BamlEditor } from './BamlEditor';

// ---------------------------------------------------------------------------
// Default BAML code
// ---------------------------------------------------------------------------

import { DEFAULT_BAML, EXAMPLE_ARGS } from './homepage-example';

// Editor pane starts narrower than the execution panel so the playground
// (results) gets the room by default; users can drag the splitter wider or
// collapse the editor entirely with the toggle on the divider.
const DEFAULT_EDITOR_PCT = 38;

// ---------------------------------------------------------------------------
// BamlPlayground — full-feature inline hero embed (desktop only)
// ---------------------------------------------------------------------------

export function BamlPlayground() {
  const [code, setCode] = useState(DEFAULT_BAML);
  const [port, setPort] = useState<RuntimePort | null>(null);
  const [connectionVersion, setConnectionVersion] = useState(0);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>(
    'loading',
  );
  const [statusMsg, setStatusMsg] = useState('Loading runtime…');

  const portRef = useRef<RuntimePort | null>(null);
  const workerRef = useRef<Worker | null>(null);
  const codeRef = useRef(code);
  codeRef.current = code;
  const respawnGenRef = useRef(0);
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Spawn (or respawn) the worker. Used both on mount and on recovery from a
  // runtime panic. The runtime crashes with OOB memory after collect_tests
  // returns a non-Handle (baml_language bex_project::multi_project::mod.rs:631);
  // when that happens the worker emits an `error` event and we restart it
  // with the user's current code so editing keeps working.
  const spawnWorker = useCallback(() => {
    const myGen = ++respawnGenRef.current;
    setStatus('loading');
    setStatusMsg('Loading runtime…');
    setPort(null);
    portRef.current?.dispose();
    portRef.current = null;
    workerRef.current?.terminate();
    workerRef.current = null;

    let worker: Worker;
    try {
      worker = new Worker(new URL('./baml-worker.ts', import.meta.url), {
        type: 'module',
        name: 'BAML Worker',
      });
    } catch (err) {
      setStatus('error');
      setStatusMsg(
        `Failed to spawn worker: ${err instanceof Error ? err.message : String(err)}`,
      );
      return;
    }
    workerRef.current = worker;

    const stale = () => respawnGenRef.current !== myGen;

    const onError = (e: ErrorEvent) => {
      if (stale()) return;
      // Runtime panic — respawn so the user can keep editing. State is
      // preserved (codeRef.current carries the latest source); ExecutionPanel
      // remounts on the new port via connectionVersion bump.
      // eslint-disable-next-line no-console
      console.warn('[BAML playground] worker died, respawning:', e.message);
      spawnWorker();
    };
    worker.addEventListener('error', onError);

    // Some panics (rejected Promises inside the worker) surface as
    // unhandledrejection rather than `error`. Treat them the same.
    const onMessageError = (e: MessageEvent) => {
      if (stale()) return;
      // eslint-disable-next-line no-console
      console.warn('[BAML playground] worker messageerror:', e);
      spawnWorker();
    };
    worker.addEventListener('messageerror', onMessageError);

    const onReady = (event: MessageEvent) => {
      if (stale()) return;
      if (event.data?.type !== 'ready') return;
      worker.removeEventListener('message', onReady);
      const newPort = new WorkerRuntimePort(worker);
      portRef.current = newPort;
      setPort(newPort);
      setConnectionVersion((v) => v + 1);
      setStatus('ready');
      setStatusMsg('Ready');
    };
    worker.addEventListener('message', onReady);

    worker.postMessage({
      type: 'init',
      initialFiles: { 'baml_src/main.baml': codeRef.current },
      rootPath: '/workspace',
    });
  }, []);

  useEffect(() => {
    spawnWorker();
    return () => {
      respawnGenRef.current++;
      portRef.current?.dispose();
      portRef.current = null;
      setPort(null);
      workerRef.current?.terminate();
      workerRef.current = null;
      if (flushTimerRef.current) clearTimeout(flushTimerRef.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Split between the editor (left %) and execution panel (right %).
  const splitContainerRef = useRef<HTMLDivElement>(null);
  const [editorPct, setEditorPct] = useState(DEFAULT_EDITOR_PCT);
  const [editorCollapsed, setEditorCollapsed] = useState(false);
  const [dragging, setDragging] = useState(false);

  const toggleEditor = useCallback(() => setEditorCollapsed((c) => !c), []);

  const onSplitPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    (e.currentTarget as Element).setPointerCapture?.(e.pointerId);
    setDragging(true);
  };

  useEffect(() => {
    if (!dragging) return;
    const onMove = (e: PointerEvent) => {
      const el = splitContainerRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const pct = ((e.clientX - rect.left) / rect.width) * 100;
      // Dragging the divider always re-opens a collapsed editor.
      setEditorCollapsed(false);
      setEditorPct(Math.max(20, Math.min(80, pct)));
    };
    const onUp = () => setDragging(false);
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
    window.addEventListener('pointercancel', onUp);
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      window.removeEventListener('pointercancel', onUp);
    };
  }, [dragging]);

  // Debounce filesChanged so rapid keystrokes coalesce into one runtime
  // didChange. Without this, fast typing fires multiple LSP notifications
  // before the runtime can drain its async work, causing mutex panics.
  const handleCodeChange = (next: string) => {
    setCode(next);
    codeRef.current = next;
    if (flushTimerRef.current) clearTimeout(flushTimerRef.current);
    flushTimerRef.current = setTimeout(() => {
      portRef.current?.postMessage({
        type: 'filesChanged',
        files: { 'baml_src/main.baml': codeRef.current },
      });
    }, 200);
  };

  return (
    <div
      ref={splitContainerRef}
      className="baml-playground-root flex h-full w-full overflow-hidden bg-vsc-bg text-vsc-text"
      style={{ cursor: dragging ? 'col-resize' : undefined }}
    >
      {/* Editor — left pane */}
      <div
        aria-hidden={editorCollapsed}
        className={`flex min-w-0 flex-col overflow-hidden ${
          dragging
            ? ''
            : 'motion-safe:transition-[width] motion-safe:duration-[180ms] motion-safe:ease-in-out'
        }`}
        style={{ width: editorCollapsed ? '0%' : `${editorPct}%` }}
      >
        <div className="flex min-h-0 flex-1 flex-col border-l border-r border-[#6D28D9]">
          <BamlEditor
            value={code}
            onChange={handleCodeChange}
            disabled={status !== 'ready'}
            chromeless
          />
        </div>
      </div>

      {/* Splitter + collapse toggle */}
      <div
        aria-label="Resize editor and execution panel"
        aria-orientation="vertical"
        aria-valuemax={80}
        aria-valuemin={20}
        aria-valuenow={editorCollapsed ? 0 : Math.round(editorPct)}
        className="group relative flex w-1 flex-shrink-0 cursor-col-resize items-center justify-center bg-vsc-border"
        onDoubleClick={() => {
          setEditorCollapsed(false);
          setEditorPct(DEFAULT_EDITOR_PCT);
        }}
        onPointerDown={onSplitPointerDown}
        role="separator"
        style={{ touchAction: 'none' }}
        tabIndex={0}
      >
        {/* Wider invisible hit area */}
        <span aria-hidden className="absolute inset-y-0 -left-1.5 -right-1.5" />
        {/* Visible accent on hover/drag */}
        <span
          aria-hidden
          className={`absolute inset-y-0 left-0 right-0 transition-colors ${
            dragging ? 'bg-[#6D28D9]' : 'group-hover:bg-[#6D28D9]/60'
          }`}
        />
        {/* Collapse / expand the editor — a click toggle alongside the drag. */}
        <button
          type="button"
          aria-label={editorCollapsed ? 'Show code editor' : 'Hide code editor'}
          title={editorCollapsed ? 'Show code' : 'Hide code'}
          onClick={(e) => {
            e.stopPropagation();
            toggleEditor();
          }}
          onPointerDown={(e) => e.stopPropagation()}
          onDoubleClick={(e) => e.stopPropagation()}
          className="absolute top-2.5 z-10 flex h-6 w-6 cursor-pointer items-center justify-center rounded-full border border-[#6D28D9]/40 bg-white text-[#6D28D9] shadow-sm transition-colors hover:bg-[#6D28D9] hover:text-white"
        >
          {editorCollapsed ? (
            <ChevronRight className="h-3.5 w-3.5" />
          ) : (
            <ChevronLeft className="h-3.5 w-3.5" />
          )}
        </button>
      </div>

      {/* ExecutionPanel — right pane */}
      <div className="flex min-w-0 flex-1 flex-col">
        {port ? (
          <ExecutionPanel
            key={connectionVersion}
            port={port}
            connectionVersion={connectionVersion}
            initialFunctionName="Main"
            initialArgsJson="{}"
            argsByFunction={EXAMPLE_ARGS}
          />
        ) : (
          <div className="flex h-full items-center justify-center text-xs text-vsc-text-faint">
            {status === 'error' ? statusMsg : 'Loading playground…'}
          </div>
        )}
      </div>
    </div>
  );
}
