'use client';
import { Check, Copy, X } from 'lucide-react';
import { useRef, useState } from 'react';

export function InvestigationPrompt({ prompt }: { prompt: string }) {
  const [state, setState] = useState<'idle' | 'copied' | 'failed'>('idle');
  const dialog = useRef<HTMLDialogElement>(null);
  async function copy() {
    try {
      await navigator.clipboard.writeText(prompt);
      setState('copied');
    } catch {
      setState('failed');
      dialog.current?.showModal();
    }
  }
  return (
    <div className="ml-auto flex flex-wrap items-center gap-2">
      <button
        className="inline-flex items-center gap-1.5 rounded-md border px-3 py-1.5 text-xs hover:bg-muted"
        onClick={copy}
        type="button"
      >
        {state === 'copied' ? <Check size={13} /> : <Copy size={13} />}
        {state === 'copied' ? 'Copied' : 'Copy agent prompt'}
      </button>
      <button
        className="text-xs text-muted-foreground underline"
        onClick={() => dialog.current?.showModal()}
        type="button"
      >
        Preview
      </button>
      <dialog
        aria-labelledby="prompt-title"
        className="m-auto w-[calc(100%-2rem)] max-w-2xl rounded-xl border bg-background p-0 text-foreground shadow-xl backdrop:bg-black/40"
        onClick={(event) => {
          if (event.target === event.currentTarget) dialog.current?.close();
        }}
        onKeyDown={(event) => {
          if (event.key === 'Escape') dialog.current?.close();
        }}
        ref={dialog}
      >
        <section className="space-y-3 p-5">
          <header className="flex items-center justify-between">
            <h2 className="font-semibold" id="prompt-title">
              Agent prompt
            </h2>
            <button
              aria-label="Close prompt"
              onClick={() => dialog.current?.close()}
              type="button"
            >
              <X size={18} />
            </button>
          </header>
          <p className="text-xs text-muted-foreground">
            Toolchain setup, source findings and repro files.
          </p>
          {state === 'failed' && (
            <output className="text-sm">
              Select and copy the prompt below.
            </output>
          )}
          <textarea
            aria-label="Investigation prompt"
            className="h-[50vh] w-full rounded-md border bg-muted/30 p-3 font-mono text-xs leading-5"
            readOnly
            value={prompt}
          />
        </section>
      </dialog>
    </div>
  );
}
