'use client';

import dynamic from 'next/dynamic';
import { useCallback, useState } from 'react';
import { Highlight } from '@/components/home/code';
import type { BamlEditorProps } from './baml-editor';

// A block of BAML that becomes a live editor (./baml-editor.tsx). The editor is Monaco and the
// BAML runtime, a WebAssembly build of the compiler and the VM: several megabytes together, and
// neither can render on the server. So the block is first drawn as plain highlighted code, at the
// size the editor will be, and the editor replaces it when the block nears the window. A reader
// with scripts off, or who never scrolls that far, has the code and nothing was fetched for it.
//
// Each editor's runtime is a worker of its own. An editor stays on the page once it is there, but
// its runtime is ended when the block is well away from the window and started again when it comes
// back, so a long page runs two or three workers at a time, not one for every sample on it.

const BamlEditor = dynamic(() => import('./baml-editor'), { ssr: false });

/** The height the editor gives this code: twenty pixels a line, and its padding. */
const heightOf = (code: string, max: number) =>
  Math.min(max, code.split('\n').length * 20 + 24);

/** BAML source that turns into a live editor as it nears the window. */
export function LiveCode({
  code,
  filename,
  runnable = true,
  maxHeight = 900,
}: Omit<BamlEditorProps, 'awake'>) {
  // Whether the block is near the window now, and whether it ever has been.
  const [near, setNear] = useState(false);
  const [seen, setSeen] = useState(false);
  const watch = useCallback((node: HTMLDivElement | null) => {
    if (!node) return undefined;
    // "Near" is within a screen and a half of the window. The margin is in pixels of the
    // window's height: browsers have disagreed on what a percentage margin is relative to.
    const margin = Math.round(window.innerHeight * 1.5);
    const observer = new IntersectionObserver(
      ([entry]) => {
        setNear(entry.isIntersecting);
        if (entry.isIntersecting) setSeen(true);
      },
      { rootMargin: `${margin}px 0px` },
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  const source = code.trimEnd();
  return (
    <div className="not-prose my-7" ref={watch}>
      {seen ? (
        <BamlEditor
          awake={near}
          code={source}
          filename={filename}
          maxHeight={maxHeight}
          runnable={runnable}
        />
      ) : (
        <div className="overflow-hidden rounded-xl border border-rule bg-[#121212]">
          <div className="flex h-10 items-center border-b border-rule pl-4">
            <span className="font-mono text-[12px] text-body">{filename}</span>
          </div>
          <pre
            className="overflow-hidden px-4 py-3 font-mono text-[13px] leading-5 text-[#BDBDC4]"
            style={{ height: heightOf(source, maxHeight) }}
          >
            {source.split('\n').map((line, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: a sample's lines keep their positions
              <div className="whitespace-pre-wrap" key={i}>
                {line ? <Highlight line={line} /> : ' '}
              </div>
            ))}
          </pre>
        </div>
      )}
    </div>
  );
}
