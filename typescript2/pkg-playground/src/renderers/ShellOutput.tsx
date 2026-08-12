/**
 * Renders a baml.sys.ShellOutput.
 *
 * Inline (event log):  compact one-line summary, click to expand with stdout/stderr tabs.
 * Expanded (result panel): class name header, exit_code, stdout/stderr tabs.
 */

import { useState, type FC } from 'react';
import { ChevronRight } from 'lucide-react';
import { CopyButton } from '../components/CopyButton';
import { ToggleGroup } from '../components/ui/toggle-group';
import type { ResultRendererProps } from '../result-renderers';

interface ShellOutputShape {
  stdout?: unknown;
  stderr?: unknown;
  exit_code?: number;
}

function isShellOutput(value: unknown): value is ShellOutputShape {
  if (value == null || typeof value !== 'object') return false;
  const o = value as Record<string, unknown>;
  return 'exit_code' in o;
}

/** Decode a bytes-like value to a UTF-8 string for display.
 *  Handles tagged base64 `{ $baml: { type: "$bytes" }, base64: "..." }`,
 *  native Uint8Array, and string fallback. */
function decodeBytes(bytes: unknown): string {
  if (bytes == null) return '';
  // Tagged base64 from JSON round-trip through worker
  if (typeof bytes === 'object' && !Array.isArray(bytes)) {
    const obj = bytes as Record<string, unknown>;
    const baml = obj.$baml as Record<string, unknown> | undefined;
    if (baml?.type === '$bytes' && typeof obj.base64 === 'string') {
      const binary = atob(obj.base64);
      const arr = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) arr[i] = binary.charCodeAt(i);
      return new TextDecoder('utf-8', { fatal: false }).decode(arr);
    }
  }
  // Native Uint8Array (direct protobuf path)
  if (bytes instanceof Uint8Array) {
    return new TextDecoder('utf-8', { fatal: false }).decode(bytes);
  }
  // Fallback: if it's somehow a string already
  if (typeof bytes === 'string') return bytes;
  return '';
}

/** Compact description for inline mode: "shell exit 0. stdout: 5 lines." */
function inlineSummary(code: number, stdout: string, stderr: string): string {
  const parts = [`shell exit ${code}.`];
  const stdoutLines = stdout ? stdout.split('\n').filter(Boolean).length : 0;
  const stderrLines = stderr ? stderr.split('\n').filter(Boolean).length : 0;
  if (stdoutLines > 0)
    parts.push(
      `stdout: ${stdoutLines} ${stdoutLines === 1 ? 'line' : 'lines'}.`,
    );
  if (stderrLines > 0)
    parts.push(
      `stderr: ${stderrLines} ${stderrLines === 1 ? 'line' : 'lines'}.`,
    );
  if (stdoutLines === 0 && stderrLines === 0) parts.push('no output.');
  return parts.join(' ');
}

type OutputTab = 'stdout' | 'stderr';

const OutputTabs: FC<{ stdout: string; stderr: string }> = ({
  stdout,
  stderr,
}) => {
  const [tab, setTab] = useState<OutputTab>('stdout');
  const content = tab === 'stdout' ? stdout : stderr;
  const empty = !content;
  const isErr = tab === 'stderr';

  return (
    <div>
      <div className="flex items-center gap-1 mb-1">
        <ToggleGroup
          value={tab}
          onValueChange={setTab}
          options={[
            { value: 'stdout' as OutputTab, label: 'stdout' },
            { value: 'stderr' as OutputTab, label: 'stderr' },
          ]}
          size="sm"
        />
        {content && <CopyButton text={content} iconSize={10} />}
      </div>
      {empty ? (
        <div className="text-[11px] text-vsc-text-faint italic pl-1">empty</div>
      ) : (
        <pre
          className={`whitespace-pre-wrap p-1.5 rounded overflow-auto max-h-[300px] m-0 text-xs ${
            isErr
              ? 'bg-red-500/5 border border-red-500/20 text-red-300'
              : 'bg-vsc-bg border border-vsc-border text-vsc-text'
          }`}
        >
          {content}
        </pre>
      )}
    </div>
  );
};

export const ShellOutputRenderer: FC<ResultRendererProps> = ({
  value,
  displayMode,
}) => {
  const [expanded, setExpanded] = useState(false);
  const shell = isShellOutput(value) ? value : null;
  if (!shell) {
    return (
      <pre className="font-vsc-mono text-xs text-vsc-text">
        {JSON.stringify(value, null, 2)}
      </pre>
    );
  }

  const ok = shell.exit_code === 0;
  const code = shell.exit_code ?? -1;
  const stdout = decodeBytes(shell.stdout).replace(/\n$/, '');
  const stderr = decodeBytes(shell.stderr).replace(/\n$/, '');
  const summary = inlineSummary(code, stdout, stderr);

  // ── Inline mode (event log rows) ──────────────────────────────────
  if (displayMode === 'inline' || displayMode === 'inline-hint') {
    return (
      <span className="font-vsc-mono text-xs inline-flex items-center gap-1.5">
        <span className="text-vsc-text-muted">baml.sys.ShellOutput</span>
        <span
          className={`px-1.5 py-0.5 rounded text-[10px] font-semibold ${ok ? 'bg-green-500/15 text-green-400' : 'bg-red-500/15 text-red-400'}`}
        >
          exit_code: {code}
        </span>
      </span>
    );
  }

  // ── Expanded mode (result panel) ──────────────────────────────────
  return (
    <div className="space-y-2 text-xs font-vsc-mono">
      {/* Header: class name + exit code */}
      <div className="flex items-center gap-2">
        <span className="text-vsc-text-muted">baml.sys.ShellOutput</span>
        <span
          className={`px-1.5 py-0.5 rounded text-[10px] font-semibold ${ok ? 'bg-green-500/15 text-green-400' : 'bg-red-500/15 text-red-400'}`}
        >
          exit_code: {code}
        </span>
      </div>
      {/* Tabbed stdout/stderr */}
      <OutputTabs stdout={stdout} stderr={stderr} />
    </div>
  );
};
