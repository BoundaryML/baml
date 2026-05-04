'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ExecutionPanel,
  WorkerRuntimePort,
  type RuntimePort,
} from '@b/pkg-playground';
import { BamlEditor } from './BamlEditor';

// ---------------------------------------------------------------------------
// Default BAML code
// ---------------------------------------------------------------------------

const DEFAULT_BAML = `// BAML Playground - Try editing and running!

client OpenAI {
  provider openai
  options {
    model "gpt-4o-mini"
    api_key env.OPENAI_API_KEY
  }
}

enum Sentiment {
  POSITIVE
  NEGATIVE
  NEUTRAL
}

function ClassifySentiment(text: string) -> Sentiment {
  client OpenAI
  prompt #"
    Classify the sentiment of the following text.

    Text: {{text}}

    Respond with exactly one of: POSITIVE, NEGATIVE, or NEUTRAL.
  "#
}

test "happy" {
  ClassifySentiment("I absolutely love this product! It exceeded all my expectations.")
}

test "sad" {
  ClassifySentiment("This was terrible. Complete waste of money and time.")
}
`;

// ---------------------------------------------------------------------------
// BamlPlayground — full-feature inline hero embed (desktop only)
// ---------------------------------------------------------------------------

export function BamlPlayground() {
  const [code, setCode] = useState(DEFAULT_BAML);
  const [port, setPort] = useState<RuntimePort | null>(null);
  const [connectionVersion, setConnectionVersion] = useState(0);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
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
      setStatusMsg(`Failed to spawn worker: ${err instanceof Error ? err.message : String(err)}`);
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
    <div className="baml-playground-root flex h-full w-full overflow-hidden bg-vsc-bg text-vsc-text">
      {/* Editor — left half */}
      <div className="flex w-1/2 min-w-0 flex-col border-r border-vsc-border">
        <BamlEditor
          value={code}
          onChange={handleCodeChange}
          disabled={status !== 'ready'}
          chromeless
        />
      </div>

      {/* ExecutionPanel — right half */}
      <div className="flex w-1/2 min-w-0 flex-col">
        {port ? (
          <ExecutionPanel
            key={connectionVersion}
            port={port}
            connectionVersion={connectionVersion}
            initialFunctionName="ClassifySentiment"
            initialArgsJson='{"text":"baml rocks"}'
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
