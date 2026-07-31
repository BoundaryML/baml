import { Loader2, Play } from 'lucide-react';
import {
  type FC,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';

import {
  type BqlQueryResult,
  type BqlSchema,
  type BqlStageSpec,
  decodeBqlFrame,
  decodeBqlSchemaFrame,
  WsObserveClient,
} from './observe-client';

const QUERY_MAX_BYTES = 512 * 1024;
const DEFAULT_QUERY = 'runs(limit=50)';

export const BqlStudio: FC<{ client: WsObserveClient }> = ({ client }) => {
  const editorRef = useRef<HTMLTextAreaElement>(null);
  const [source, setSource] = useState(DEFAULT_QUERY);
  const [schema, setSchema] = useState<BqlSchema | null>(null);
  const [results, setResults] = useState<BqlQueryResult[]>([]);
  const [selectedStage, setSelectedStage] = useState<BqlStageSpec | null>(null);
  const [completionOpen, setCompletionOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    void client
      .query({ kind: 'bqlSchema', maxBytes: QUERY_MAX_BYTES })
      .then(decodeBqlSchemaFrame)
      .then((next) => {
        if (disposed) return;
        setSchema(next);
        setSelectedStage(next.stages.find((stage) => stage.name === 'runs') ?? null);
      })
      .catch((cause) => {
        if (!disposed) setError(errorMessage(cause));
      });
    return () => {
      disposed = true;
    };
  }, [client]);

  const runQuery = useCallback(
    async (cursor?: string, snapshot?: string) => {
      setLoading(true);
      setCompletionOpen(false);
      try {
        const frame = await client.query({
          kind: 'bql',
          source,
          maxRows: schema?.default_limit ?? 1000,
          maxBytes: QUERY_MAX_BYTES,
          cursor,
          snapshot,
        });
        setResults(decodeBqlFrame(frame));
        setError(null);
      } catch (cause) {
        setResults([]);
        setError(errorMessage(cause));
      } finally {
        setLoading(false);
      }
    },
    [client, schema?.default_limit, source],
  );

  const completionPrefix = useMemo(() => {
    const editor = editorRef.current;
    const caret = editor?.selectionStart ?? source.length;
    return source.slice(0, caret).match(/[a-z_][a-z0-9_]*$/i)?.[0] ?? '';
  }, [source, completionOpen]);
  const completions = useMemo(
    () =>
      (schema?.stages ?? [])
        .filter((stage) =>
          completionPrefix === ''
            ? stage.availability === 'implemented'
            : stage.name.startsWith(completionPrefix.toLowerCase()),
        )
        .slice(0, 10),
    [completionPrefix, schema?.stages],
  );

  const insertStage = useCallback(
    (stage: BqlStageSpec) => {
      const editor = editorRef.current;
      const selectionStart = editor?.selectionStart ?? source.length;
      const selectionEnd = editor?.selectionEnd ?? selectionStart;
      const prefixStart = Math.max(0, selectionStart - completionPrefix.length);
      const required = stage.arguments
        .filter((argument) => argument.required)
        .map((argument) => argument.example)
        .join(', ');
      const snippet = `${stage.name}(${required})`;
      const next =
        source.slice(0, prefixStart) + snippet + source.slice(selectionEnd);
      setSource(next);
      setSelectedStage(stage);
      setCompletionOpen(false);
      requestAnimationFrame(() => {
        const caret = prefixStart + snippet.length;
        editor?.focus();
        editor?.setSelectionRange(caret, caret);
      });
    },
    [completionPrefix.length, source],
  );

  const onEditorKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') {
      event.preventDefault();
      void runQuery();
    } else if (event.ctrlKey && event.key === ' ') {
      event.preventDefault();
      setCompletionOpen(true);
    } else if (event.key === 'Escape') {
      setCompletionOpen(false);
    }
  };

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[minmax(320px,42%)_minmax(420px,1fr)] bg-vsc-bg font-vsc-mono text-xs">
      <section className="flex min-h-0 min-w-0 flex-col border-r border-vsc-border">
        <div className="flex h-8 shrink-0 items-center border-b border-vsc-border bg-vsc-surface px-2">
          <span className="font-semibold text-vsc-text">BQL query</span>
          <span className="ml-2 text-[10px] text-vsc-text-faint">
            Ctrl/⌘ Enter to run · Ctrl Space to complete
          </span>
          <button
            className="ml-auto flex h-6 items-center gap-1 rounded border border-vsc-border bg-vsc-button-bg px-2 text-vsc-text hover:bg-vsc-button-hover disabled:opacity-50"
            disabled={loading || source.trim() === ''}
            onClick={() => void runQuery()}
            type="button"
          >
            {loading ? (
              <Loader2 className="h-3 w-3 animate-spin" />
            ) : (
              <Play className="h-3 w-3" />
            )}
            Run
          </button>
        </div>
        <div className="relative min-h-44 shrink-0 border-b border-vsc-border">
          <textarea
            aria-label="BQL query editor"
            className="h-full min-h-44 w-full resize-none border-0 bg-vsc-bg p-3 font-vsc-mono text-xs leading-5 text-vsc-text outline-none"
            onChange={(event) => {
              setSource(event.target.value);
              setCompletionOpen(true);
            }}
            onClick={() => setCompletionOpen(true)}
            onKeyDown={onEditorKeyDown}
            ref={editorRef}
            spellCheck={false}
            value={source}
          />
          {completionOpen && completions.length > 0 && (
            <div className="absolute inset-x-3 top-16 z-20 max-h-48 overflow-auto rounded border border-vsc-border bg-vsc-surface shadow-lg">
              {completions.map((stage) => (
                <button
                  className="flex w-full items-start gap-2 border-0 border-b border-vsc-border-subtle bg-transparent px-2 py-1.5 text-left hover:bg-vsc-list-hover"
                  key={stage.name}
                  onClick={() => insertStage(stage)}
                  onMouseEnter={() => setSelectedStage(stage)}
                  title={stage.description}
                  type="button"
                >
                  <span className="font-semibold text-vsc-accent">{stage.name}</span>
                  <span className="min-w-0 flex-1 truncate text-vsc-text-muted">
                    {stage.description}
                  </span>
                  {stage.availability !== 'implemented' && (
                    <span className="text-[10px] text-vsc-yellow">unavailable</span>
                  )}
                </button>
              ))}
            </div>
          )}
        </div>
        <div className="min-h-0 flex-1 overflow-auto">
          <div className="sticky top-0 border-b border-vsc-border bg-vsc-surface px-2 py-1.5 font-semibold text-vsc-text">
            Catalog {schema == null ? 'loading…' : `v${schema.version}`}
          </div>
          <div className="flex flex-wrap gap-1 border-b border-vsc-border p-2">
            {(schema?.stages ?? []).map((stage) => (
              <button
                className={`rounded border px-1.5 py-0.5 ${
                  selectedStage?.name === stage.name
                    ? 'border-vsc-accent bg-vsc-list-active text-vsc-text'
                    : 'border-vsc-border bg-transparent text-vsc-text-muted hover:bg-vsc-list-hover'
                } ${
                  stage.availability === 'implemented'
                    ? ''
                    : 'border-dashed opacity-70'
                }`}
                key={stage.name}
                onClick={() => insertStage(stage)}
                onMouseEnter={() => setSelectedStage(stage)}
                title={`${stage.name}: ${stage.description}`}
                type="button"
              >
                {stage.name}
              </button>
            ))}
          </div>
          {selectedStage != null && (
            <div className="space-y-2 p-3 text-vsc-text-muted">
              <div className="flex items-center gap-2">
                <code className="font-semibold text-vsc-accent">
                  {stageSignature(selectedStage)}
                </code>
                <span className="rounded bg-vsc-list-hover px-1 text-[10px]">
                  {selectedStage.output}
                </span>
              </div>
              <p>{selectedStage.description}</p>
              {selectedStage.arguments.map((argument) => (
                <div className="grid grid-cols-[90px_1fr] gap-2" key={argument.name}>
                  <code className="text-vsc-text">{argument.name}</code>
                  <span>
                    {argument.value_type}
                    {argument.default != null ? ` · default ${argument.default}` : ''}
                    {argument.enum_values.length > 0
                      ? ` · ${argument.enum_values.join(' | ')}`
                      : ''}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </section>
      <section className="flex min-h-0 min-w-0 flex-col">
        <div className="flex h-8 shrink-0 items-center border-b border-vsc-border bg-vsc-surface px-2">
          <span className="font-semibold text-vsc-text">Results</span>
          <span className="ml-auto text-[10px] text-vsc-text-faint">
            BQF1 · max {formatBytes(QUERY_MAX_BYTES)}
          </span>
        </div>
        {error != null && (
          <div className="border-b border-vsc-error/30 bg-vsc-error/10 px-3 py-2 text-vsc-error">
            {error}
          </div>
        )}
        <div className="min-h-0 flex-1 overflow-auto">
          {results.length === 0 && error == null ? (
            <div className="flex h-full items-center justify-center p-6 text-center text-vsc-text-faint">
              Run a query to inspect rows and completeness metadata.
            </div>
          ) : (
            results.map((result, index) => (
              <BqlResultTable
                key={`${result.name ?? 'result'}-${index}`}
                onNextPage={(cursor, snapshot) =>
                  void runQuery(cursor, snapshot)
                }
                result={result}
              />
            ))
          )}
        </div>
      </section>
    </div>
  );
};

const BqlResultTable: FC<{
  result: BqlQueryResult;
  onNextPage(cursor: string, snapshot: string): void;
}> = ({ result, onNextPage }) => (
  <article className="border-b border-vsc-border">
    <div className="flex h-8 items-center border-b border-vsc-border-subtle bg-vsc-surface px-2">
      <span className="font-semibold text-vsc-text">
        {result.name ?? 'result'}
      </span>
      <span className="ml-2 text-[10px] text-vsc-text-faint">
        {result.rows.length} rows · {result.kind}
      </span>
      <span
        className={`ml-auto rounded px-1.5 py-0.5 text-[10px] ${
          result.meta.complete
            ? 'bg-vsc-green/15 text-vsc-green'
            : 'bg-vsc-yellow/15 text-vsc-yellow'
        }`}
      >
        {result.meta.complete ? 'complete' : 'partial'}
      </span>
    </div>
    {result.columns.length === 0 ? (
      <div className="p-3 text-vsc-text-faint">No columns</div>
    ) : (
      <div className="overflow-auto">
        <table className="min-w-full border-collapse text-left">
          <thead className="sticky top-0 bg-vsc-surface text-vsc-text-muted">
            <tr>
              {result.columns.map((column) => (
                <th
                  className="whitespace-nowrap border-b border-r border-vsc-border px-2 py-1 font-semibold"
                  key={column}
                >
                  {column}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {result.rows.map((row, rowIndex) => (
              <tr
                className="border-b border-vsc-border-subtle hover:bg-vsc-list-hover"
                key={rowIndex}
              >
                {result.columns.map((column) => (
                  <td
                    className="max-w-72 whitespace-nowrap border-r border-vsc-border-subtle px-2 py-1 text-vsc-text"
                    key={column}
                    title={renderValue(row[column])}
                  >
                    <span className="block max-w-72 truncate">
                      {renderValue(row[column])}
                    </span>
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    )}
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 bg-vsc-surface px-2 py-1.5 text-[10px] text-vsc-text-muted">
      {result.meta.truncated && <span className="text-vsc-yellow">truncated</span>}
      <span>{result.meta.sources_consulted.length} sources</span>
      <span>{result.meta.watermarks.length} watermarks</span>
      {result.meta.capture_loss.length > 0 && (
        <span className="text-vsc-error">
          {result.meta.capture_loss.length} capture-loss records
        </span>
      )}
      <span className="max-w-52 truncate" title={result.meta.snapshot}>
        snapshot {result.meta.snapshot}
      </span>
      {result.meta.next_cursor != null && (
        <button
          className="rounded border border-vsc-border px-1.5 py-0.5 text-vsc-text hover:bg-vsc-list-hover"
          onClick={() =>
            onNextPage(result.meta.next_cursor!, result.meta.snapshot)
          }
          type="button"
        >
          Next page
        </button>
      )}
      {result.meta.warnings.map((warning) => (
        <span className="basis-full text-vsc-yellow" key={warning}>
          {warning}
        </span>
      ))}
    </div>
  </article>
);

function stageSignature(stage: BqlStageSpec): string {
  return `${stage.name}(${stage.arguments
    .map((argument) => `${argument.name}${argument.required ? '' : '?'}: ${argument.value_type}`)
    .join(', ')})`;
}

function renderValue(value: unknown): string {
  if (value == null) return 'null';
  if (typeof value === 'string') return value;
  if (
    typeof value === 'number' ||
    typeof value === 'boolean' ||
    typeof value === 'bigint'
  ) {
    return String(value);
  }
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function formatBytes(bytes: number): string {
  return `${Math.round(bytes / 1024)} KiB`;
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? `${cause.name}: ${cause.message}` : String(cause);
}
