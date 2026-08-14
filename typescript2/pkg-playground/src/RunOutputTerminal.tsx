/**
 * RunOutputTerminal — renders a run's `baml.io` stream writes.
 *
 * This is a real terminal emulator, not a log list, because BAML programs
 * print real terminal output: SGR color, cursor movement, `\r` progress bars,
 * `\x1b[2J` screen clears. Those need a screen buffer to interpret, which an
 * escape-to-HTML converter cannot provide.
 *
 * xterm.js is fed chunks verbatim and in order. It buffers partial escape
 * sequences across `write()` calls, so a sequence split across two `print`
 * calls still resolves correctly.
 *
 * It lives inline in the run strip alongside INPUT/RESULT, so it sizes itself
 * to the output it holds rather than filling its parent: short output gets a
 * short box, and past MAX_ROWS it stops growing and scrolls internally.
 */
import type { FC } from 'react';
import { useEffect, useRef, useState } from 'react';

import type { RunOutputChunk } from './run-store-projections';

import '@xterm/xterm/css/xterm.css';

const FONT_SIZE = 12;
const LINE_HEIGHT = 1.4;
const ROW_PX = Math.ceil(FONT_SIZE * LINE_HEIGHT);
const MIN_ROWS = 3;
const MAX_ROWS = 24;

type TerminalHandle = {
  write: (data: string, callback?: () => void) => void;
  reset: () => void;
  dispose: () => void;
  buffer: { active: { length: number } };
};

export type RunOutputTerminalProps = {
  chunks: RunOutputChunk[];
  /** Reset the screen when this changes (i.e. when a different run is shown). */
  runKey: string | null;
};

export const RunOutputTerminal: FC<RunOutputTerminalProps> = ({ chunks, runKey }) => {
  const hostRef = useRef<HTMLDivElement | null>(null);
  // Loaded lazily so xterm stays out of the bundle until a run actually
  // prints, and so this module remains importable during SSR.
  const termRef = useRef<TerminalHandle | null>(null);
  const fitRef = useRef<{ fit: () => void } | null>(null);
  // How many chunks have already been written, so re-renders append rather
  // than replay. Reset alongside the terminal when the run changes.
  const writtenRef = useRef(0);
  const pendingRef = useRef<string[]>([]);
  const [rows, setRows] = useState(MIN_ROWS);

  useEffect(() => {
    let disposed = false;
    const host = hostRef.current;
    if (!host) return;

    void (async () => {
      const [{ Terminal }, { FitAddon }] = await Promise.all([
        import('@xterm/xterm'),
        import('@xterm/addon-fit'),
      ]);
      if (disposed) return;

      const term = new Terminal({
        convertEol: true,
        cursorBlink: false,
        cursorStyle: 'bar',
        disableStdin: true,
        fontSize: FONT_SIZE,
        lineHeight: LINE_HEIGHT,
        fontFamily:
          'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace',
        scrollback: 10_000,
        theme: { background: '#00000000' },
        allowTransparency: true,
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.open(host);
      fit.fit();

      termRef.current = term as unknown as TerminalHandle;
      fitRef.current = fit;
      for (const text of pendingRef.current) term.write(text);
      pendingRef.current = [];
    })();

    return () => {
      disposed = true;
      termRef.current?.dispose();
      termRef.current = null;
      fitRef.current = null;
      // Rewind the write cursor with the terminal. Refs outlive the emulator
      // (React StrictMode double-invokes effects in dev, and any parent
      // remount does the same), so leaving the cursor at N would replay
      // nothing into the fresh, empty screen and the user would see output
      // that starts mid-stream. Pending writes go too: they were already
      // flushed into the instance we just disposed.
      writtenRef.current = 0;
      pendingRef.current = [];
    };
  }, []);

  // Keep the emulator sized to its container; a wrong column count wraps
  // output in places the program never intended.
  useEffect(() => {
    const host = hostRef.current;
    if (!host || typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(() => fitRef.current?.fit());
    observer.observe(host);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    termRef.current?.reset();
    writtenRef.current = 0;
    pendingRef.current = [];
    setRows(MIN_ROWS);
  }, [runKey]);

  useEffect(() => {
    // A shorter list than we've written means the run was replaced by an
    // older/other one; start over rather than dropping the leading output.
    if (chunks.length < writtenRef.current) {
      termRef.current?.reset();
      writtenRef.current = 0;
      pendingRef.current = [];
    }
    const first = writtenRef.current;
    for (let i = first; i < chunks.length; i++) {
      const text = chunks[i]!.text;
      if (termRef.current) {
        // Grow to fit the buffer once the parser has caught up. `write` is
        // async (the callback fires "when the data was processed by the
        // parser"), so measuring right after the call would size against the
        // previous frame's buffer and lag one update behind. Reading the
        // emulator's own line count rather than counting newlines keeps this
        // honest for output that moves the cursor or clears the screen
        // instead of appending lines.
        const isLast = i === chunks.length - 1;
        termRef.current.write(
          text,
          isLast
            ? () => {
                const used = termRef.current?.buffer.active.length;
                if (used == null) return;
                const next = Math.min(MAX_ROWS, Math.max(MIN_ROWS, used));
                setRows((prev) => (prev === next ? prev : next));
              }
            : undefined,
        );
      } else {
        pendingRef.current.push(text);
      }
    }
    writtenRef.current = chunks.length;
  }, [chunks]);

  return (
    <div
      ref={hostRef}
      className="w-full overflow-hidden"
      style={{ height: rows * ROW_PX }}
    />
  );
};
