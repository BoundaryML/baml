'use client';

import { useState, type ReactNode } from 'react';

// Line-prefix → CSS class, matching the markers emitted by
// bench_core.transcript.render_terminal_transcript (Claude-Code-terminal style).
const MARKERS: Array<[string, string]> = [
  ['⏺ ', 't-asst'],
  ['✻', 't-think'],
  ['  ⎿', 't-result'],
  ['> ', 't-user'],
];

/**
 * Renders the raw transcript as a colorized, Ctrl-F-able terminal: each line is
 * classed by its leading marker (assistant / thinking / tool result / user), and
 * continuation lines inherit the current block's color until a blank line.
 * @param text - the rendered terminal transcript
 */
function TerminalView({ text }: { text: string }) {
  let mode = '';
  return (
    <div className="terminal">
      {text.split('\n').map((line, i) => {
        if (line === '') {
          mode = '';
        } else {
          const hit = MARKERS.find(([m]) => line.startsWith(m));
          if (hit) mode = hit[1];
        }
        // Claude Code's signature green ⏺ bullet on assistant/tool-call lines.
        const content: ReactNode = line.startsWith('⏺ ') ? (
          <>
            <span className="t-dot">⏺</span>
            {line.slice(1)}
          </>
        ) : (
          line || ' '
        );
        return (
          <div key={i} className={mode || undefined}>
            {content}
          </div>
        );
      })}
    </div>
  );
}

// Toggle between the structured per-call turn log and the raw terminal transcript.
// The raw view is the full conversation rendered Claude-Code-style, so the
// browser's native Ctrl-F searches everything; the structured view stays default.
/**
 * Client component that switches the transcript section between the structured
 * turn log and the raw, Ctrl-F-able terminal transcript. When no raw transcript
 * is available it renders the structured view alone (no toggle).
 * @param structured - the server-rendered structured turn blocks
 * @param raw - the raw terminal transcript, or null when unavailable
 * @returns the toggle (when raw exists) and the active view
 */
export default function TranscriptTabs({
  structured,
  raw,
}: {
  structured: ReactNode;
  raw: string | null;
}) {
  const [mode, setMode] = useState<'structured' | 'raw'>('structured');
  if (!raw) return <>{structured}</>;

  return (
    <div>
      <div className="seg" role="tablist" style={{ marginBottom: 12 }}>
        <button
          className={`linkbtn${mode === 'structured' ? ' seg-on' : ''}`}
          onClick={() => setMode('structured')}
        >
          structured
        </button>
        <button
          className={`linkbtn${mode === 'raw' ? ' seg-on' : ''}`}
          onClick={() => setMode('raw')}
        >
          terminal
        </button>
      </div>
      {mode === 'structured' ? structured : <TerminalView text={raw} />}
    </div>
  );
}
