import { RefreshCw } from 'lucide-react';
import {
  type FC,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { BqlStudio } from './BqlStudio';
import {
  decodeDiffFrame,
  decodeLeftHeavyFrame,
  decodeRunsFrame,
  decodeSandwichFrame,
  decodeSearchFrame,
  decodeValueDagFrame,
  decodeValueRefsFrame,
  type LeftHeavyRow,
  type ObserveDiffRow,
  type ObserveRun,
  type ObserveSearchRow,
  type ObserveValueDagRow,
  type ObserveValueRef,
  type SandwichRow,
  WsObserveClient,
} from './observe-client';

const RUN_STATE_LABELS = [
  'missing metadata',
  'begun',
  'bound',
  'running',
  'crashed',
  'complete',
  'partial / loss',
] as const;

type ValueSelection = {
  boundaryId: string;
  cid: string;
  label: string;
};

export const ObserveRunsView: FC = () => {
  const client = useMemo(() => new WsObserveClient(), []);
  const [activeView, setActiveView] = useState<'runs' | 'studio'>('runs');
  const [runs, setRuns] = useState<ObserveRun[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [leftHeavy, setLeftHeavy] = useState<LeftHeavyRow[]>([]);
  const [sandwich, setSandwich] = useState<SandwichRow[]>([]);
  const [valueRefs, setValueRefs] = useState<ObserveValueRef[]>([]);
  const [valueRows, setValueRows] = useState<ObserveValueDagRow[]>([]);
  const [valueInspection, setValueInspection] = useState<ValueSelection | null>(
    null,
  );
  const [valueDiffLeft, setValueDiffLeft] = useState<ValueSelection | null>(
    null,
  );
  const [valueDiffRight, setValueDiffRight] = useState<ValueSelection | null>(
    null,
  );
  const [valueRowsTruncated, setValueRowsTruncated] = useState(false);
  const [valueRowsAreDiff, setValueRowsAreDiff] = useState(false);
  const [searchText, setSearchText] = useState('');
  const [searchRows, setSearchRows] = useState<ObserveSearchRow[]>([]);
  const [diffRightId, setDiffRightId] = useState<string | null>(null);
  const [diffRows, setDiffRows] = useState<ObserveDiffRow[]>([]);
  const [selectedFunctionId, setSelectedFunctionId] = useState<number | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  const [loadingTree, setLoadingTree] = useState(false);
  const [loadingAdvanced, setLoadingAdvanced] = useState(false);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void client
      .subscribe(
        { kind: 'runs', limit: 250, maxBytes: 200 * 1024 },
        (frame) => {
          if (disposed) return;
          try {
            const next = decodeRunsFrame(frame);
            setRuns(next);
            setSelectedId((current) => current ?? next[0]?.boundaryId ?? null);
            setError(null);
          } catch (cause) {
            setError(errorMessage(cause));
          }
        },
        2,
      )
      .then((subscription) => {
        if (disposed) subscription.unsubscribe();
        else unsubscribe = subscription.unsubscribe;
      })
      .catch((cause) => setError(errorMessage(cause)));
    return () => {
      disposed = true;
      unsubscribe?.();
      client.close();
    };
  }, [client]);

  const loadLeftHeavy = useCallback(
    async (boundaryId: string) => {
      setSelectedId(boundaryId);
      setSearchRows([]);
      setDiffRows([]);
      setValueRows([]);
      setValueInspection(null);
      setLoadingTree(true);
      try {
        const frame = await client.query({
          boundaryId,
          kind: 'leftHeavy',
          maxBytes: 200 * 1024,
          pixelWidth: 1600,
        });
        const rows = decodeLeftHeavyFrame(frame);
        setLeftHeavy(rows);
        const functionId =
          rows.find((row) => !row.syntheticSmaller)?.functionId ?? null;
        setSelectedFunctionId(functionId);
        const valueFrame = await client.query({
          boundaryId,
          kind: 'valueRefs',
          maxBytes: 200 * 1024,
          maxRows: 250,
        });
        setValueRefs(decodeValueRefsFrame(valueFrame));
        if (functionId != null) {
          const sandwichFrame = await client.query({
            boundaryId,
            calleeDepth: 8,
            callerDepth: 8,
            functionId,
            kind: 'sandwich',
            maxBytes: 200 * 1024,
            maxRows: 250,
          });
          setSandwich(decodeSandwichFrame(sandwichFrame));
        } else {
          setSandwich([]);
        }
        setError(null);
      } catch (cause) {
        setLeftHeavy([]);
        setSandwich([]);
        setValueRefs([]);
        setError(errorMessage(cause));
      } finally {
        setLoadingTree(false);
      }
    },
    [client],
  );

  const loadSandwich = useCallback(
    async (functionId: number) => {
      if (selectedId == null) return;
      setSelectedFunctionId(functionId);
      try {
        const frame = await client.query({
          boundaryId: selectedId,
          calleeDepth: 8,
          callerDepth: 8,
          functionId,
          kind: 'sandwich',
          maxBytes: 200 * 1024,
          maxRows: 250,
        });
        setSandwich(decodeSandwichFrame(frame));
      } catch (cause) {
        setError(errorMessage(cause));
      }
    },
    [client, selectedId],
  );

  const searchFunctions = useCallback(async () => {
    if (selectedId == null) return;
    setLoadingAdvanced(true);
    try {
      const frame = await client.query({
        boundaryId: selectedId,
        kind: 'search',
        maxBytes: 200 * 1024,
        maxRows: 100,
        text: searchText,
      });
      setSearchRows(decodeSearchFrame(frame));
      setError(null);
    } catch (cause) {
      setSearchRows([]);
      setError(errorMessage(cause));
    } finally {
      setLoadingAdvanced(false);
    }
  }, [client, searchText, selectedId]);

  const comparisonRuns = runs.filter((run) => run.boundaryId !== selectedId);
  const comparisonId = comparisonRuns.some(
    (run) => run.boundaryId === diffRightId,
  )
    ? diffRightId
    : (comparisonRuns[0]?.boundaryId ?? null);

  const loadDiff = useCallback(async () => {
    if (selectedId == null || comparisonId == null) return;
    setLoadingAdvanced(true);
    try {
      const frame = await client.query({
        kind: 'diff',
        leftBoundaryId: selectedId,
        maxBytes: 200 * 1024,
        maxRows: 250,
        rightBoundaryId: comparisonId,
      });
      setDiffRows(decodeDiffFrame(frame));
      setError(null);
    } catch (cause) {
      setDiffRows([]);
      setError(errorMessage(cause));
    } finally {
      setLoadingAdvanced(false);
    }
  }, [client, comparisonId, selectedId]);

  const inspectValue = useCallback(
    async (selection: ValueSelection) => {
      setLoadingAdvanced(true);
      try {
        const frame = await client.query({
          boundaryId: selection.boundaryId,
          kind: 'valueDag',
          maxBytes: 256 * 1024,
          maxDepth: 2,
          maxNodes: 256,
          rootCid: selection.cid,
        });
        setValueRows(decodeValueDagFrame(frame));
        setValueRowsTruncated((frame.flags & (1 << 3)) !== 0);
        setValueRowsAreDiff(false);
        setValueInspection(selection);
        setError(null);
      } catch (cause) {
        setValueRows([]);
        setError(errorMessage(cause));
      } finally {
        setLoadingAdvanced(false);
      }
    },
    [client],
  );

  const compareValues = useCallback(async () => {
    if (valueDiffLeft == null || valueDiffRight == null) return;
    setLoadingAdvanced(true);
    try {
      const frame = await client.query({
        kind: 'valueDiff',
        leftBoundaryId: valueDiffLeft.boundaryId,
        leftRootCid: valueDiffLeft.cid,
        maxBytes: 256 * 1024,
        maxNodes: 256,
        rightBoundaryId: valueDiffRight.boundaryId,
        rightRootCid: valueDiffRight.cid,
      });
      setValueRows(decodeValueDagFrame(frame));
      setValueRowsTruncated((frame.flags & (1 << 3)) !== 0);
      setValueRowsAreDiff(true);
      setValueInspection(null);
      setError(null);
    } catch (cause) {
      setValueRows([]);
      setError(errorMessage(cause));
    } finally {
      setLoadingAdvanced(false);
    }
  }, [client, valueDiffLeft, valueDiffRight]);

  const functionOptions = Array.from(
    new Set(
      leftHeavy
        .filter((row) => !row.syntheticSmaller)
        .map((row) => row.functionId),
    ),
  );

  useEffect(() => {
    if (selectedId != null) void loadLeftHeavy(selectedId);
  }, [loadLeftHeavy, selectedId]);

  return (
    <div className="flex h-full min-h-0 flex-col bg-vsc-bg font-vsc-mono text-xs">
      <nav className="flex h-8 shrink-0 items-end gap-1 border-b border-vsc-border bg-vsc-surface px-2">
        {(['runs', 'studio'] as const).map((view) => (
          <button
            className={`h-7 border-x-0 border-t-0 px-3 capitalize ${
              activeView === view
                ? 'border-b-2 border-vsc-accent bg-vsc-bg text-vsc-text'
                : 'border-b-2 border-transparent bg-transparent text-vsc-text-muted hover:text-vsc-text'
            }`}
            key={view}
            onClick={() => setActiveView(view)}
            type="button"
          >
            {view}
          </button>
        ))}
        {activeView === 'studio' && (
          <span className="mb-1.5 ml-2 text-[10px] text-vsc-text-faint">
            Native BQL · schema-backed catalog
          </span>
        )}
      </nav>
      {activeView === 'studio' ? (
        <BqlStudio client={client} />
      ) : (
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(260px,32%)_minmax(400px,1fr)]">
          <section className="min-h-0 overflow-auto border-r border-vsc-border">
            <div className="sticky top-0 z-10 flex h-8 items-center border-b border-vsc-border bg-vsc-surface px-2">
              <span className="font-semibold text-vsc-text">Runs</span>
              <span className="ml-auto text-[10px] text-vsc-text-faint">
                {runs.length} recent
              </span>
            </div>
            {runs.length === 0 && error == null && (
              <div className="p-5 text-center text-vsc-text-faint">
                Waiting for observability history…
              </div>
            )}
            {runs.map((run) => (
              <button
                className={`block w-full border-0 border-b border-vsc-border-subtle px-2 py-2 text-left ${
                  run.boundaryId === selectedId
                    ? 'bg-vsc-list-active'
                    : 'bg-transparent hover:bg-vsc-list-hover'
                }`}
                key={run.boundaryId}
                onClick={() => setSelectedId(run.boundaryId)}
                type="button"
              >
                <div className="flex items-center gap-2">
                  <span
                    className={`h-1.5 w-1.5 rounded-full ${runStateColor(run.state)}`}
                  />
                  <span className="min-w-0 flex-1 truncate font-semibold text-vsc-text">
                    {run.target || '(unnamed run)'}
                  </span>
                  <span className="text-[10px] text-vsc-text-faint">
                    {formatRunTime(run.createdMs)}
                  </span>
                </div>
                <div className="mt-1 flex gap-2 pl-3.5 text-[10px] text-vsc-text-muted">
                  <span>{RUN_STATE_LABELS[run.state] ?? 'unknown'}</span>
                  {run.tornTail && (
                    <span className="text-vsc-yellow">partial tail</span>
                  )}
                  {!run.hasSnapshot && <span>live session</span>}
                </div>
              </button>
            ))}
          </section>
          <section className="flex min-h-0 min-w-0 flex-col">
            <div className="flex h-8 shrink-0 items-center border-b border-vsc-border bg-vsc-surface px-2">
              <span className="font-semibold text-vsc-text">Left Heavy</span>
              <span className="ml-2 text-[10px] text-vsc-text-faint">
                aggregate calling contexts · revision names pending
              </span>
              {(loadingTree || loadingAdvanced) && (
                <RefreshCw className="ml-auto h-3.5 w-3.5 animate-spin text-vsc-text-muted" />
              )}
            </div>
            {error != null ? (
              <div className="border-b border-vsc-error/30 bg-vsc-error/10 px-3 py-2 text-vsc-error">
                {error}
              </div>
            ) : null}
            {leftHeavy.length === 0 ? (
              <div className="flex flex-1 items-center justify-center text-vsc-text-faint">
                {selectedId == null
                  ? 'Select a run'
                  : 'No aggregate contexts available'}
              </div>
            ) : (
              <LeftHeavyCanvas rows={leftHeavy} />
            )}
            <div className="grid max-h-56 shrink-0 grid-cols-2 border-t border-vsc-border bg-vsc-surface">
              <section className="min-w-0 overflow-auto border-r border-vsc-border">
                <div className="sticky top-0 flex h-8 items-center gap-1 border-b border-vsc-border bg-vsc-surface px-2">
                  <span className="font-semibold text-vsc-text">Search</span>
                  <input
                    aria-label="Search functions"
                    className="ml-auto h-6 min-w-0 flex-1 rounded border border-vsc-border bg-vsc-input-bg px-1.5 text-vsc-text"
                    onChange={(event) => setSearchText(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') void searchFunctions();
                    }}
                    placeholder="function name"
                    value={searchText}
                  />
                  <button
                    className="h-6 rounded border border-vsc-border px-2 text-vsc-text hover:bg-vsc-list-hover disabled:opacity-50"
                    disabled={selectedId == null || loadingAdvanced}
                    onClick={() => void searchFunctions()}
                    type="button"
                  >
                    Find
                  </button>
                </div>
                {searchRows.length === 0 ? (
                  <div className="p-3 text-vsc-text-faint">
                    Search the selected run&apos;s persisted revision
                    dictionary.
                  </div>
                ) : (
                  searchRows.map((row) => (
                    <button
                      className="flex w-full items-center gap-2 border-0 border-b border-vsc-border-subtle bg-transparent px-2 py-1 text-left hover:bg-vsc-list-hover"
                      key={row.definitionKey}
                      onClick={() => void loadSandwich(row.functionId)}
                      title={row.definitionKey}
                      type="button"
                    >
                      <span className="min-w-0 flex-1 truncate text-vsc-text">
                        {row.fqn}
                      </span>
                      <span className="text-vsc-text-faint">
                        {row.calls.toString()} calls
                      </span>
                      <span className="text-vsc-text-muted">
                        {formatNs(row.totalNs)}
                      </span>
                    </button>
                  ))
                )}
              </section>
              <section className="min-w-0 overflow-auto">
                <div className="sticky top-0 flex h-8 items-center gap-1 border-b border-vsc-border bg-vsc-surface px-2">
                  <span className="font-semibold text-vsc-text">Diff</span>
                  <select
                    aria-label="Comparison run"
                    className="ml-auto h-6 min-w-0 max-w-44 bg-vsc-input-bg text-vsc-text"
                    disabled={comparisonRuns.length === 0}
                    onChange={(event) => setDiffRightId(event.target.value)}
                    value={comparisonId ?? ''}
                  >
                    {comparisonRuns.map((run) => (
                      <option key={run.boundaryId} value={run.boundaryId}>
                        {run.target || run.boundaryId}
                      </option>
                    ))}
                  </select>
                  <button
                    className="h-6 rounded border border-vsc-border px-2 text-vsc-text hover:bg-vsc-list-hover disabled:opacity-50"
                    disabled={comparisonId == null || loadingAdvanced}
                    onClick={() => void loadDiff()}
                    type="button"
                  >
                    Compare
                  </button>
                </div>
                {diffRows.length === 0 ? (
                  <div className="p-3 text-vsc-text-faint">
                    {comparisonId == null
                      ? 'Select another run to compare.'
                      : 'Compare by stable definition key across revisions.'}
                  </div>
                ) : (
                  diffRows.map((row) => (
                    <div
                      className="flex items-center gap-2 border-b border-vsc-border-subtle px-2 py-1"
                      key={row.definitionKey}
                      title={row.definitionKey}
                    >
                      <span className="min-w-0 flex-1 truncate text-vsc-text">
                        {row.fqn}
                      </span>
                      {row.presence !== 0 && (
                        <span
                          className={
                            row.presence === 1
                              ? 'text-vsc-green'
                              : 'text-vsc-error'
                          }
                        >
                          {row.presence === 1 ? 'added' : 'removed'}
                        </span>
                      )}
                      {row.definitionChanged && (
                        <span className="text-vsc-yellow">code changed</span>
                      )}
                      <span
                        className={
                          row.deltaTotalNs > 0n
                            ? 'text-vsc-error'
                            : row.deltaTotalNs < 0n
                              ? 'text-vsc-green'
                              : 'text-vsc-text-muted'
                        }
                      >
                        {formatSignedNs(row.deltaTotalNs)}
                      </span>
                    </div>
                  ))
                )}
              </section>
            </div>
            <div className="grid max-h-56 shrink-0 grid-cols-2 border-t border-vsc-border bg-vsc-surface">
              <section className="min-w-0 overflow-auto border-r border-vsc-border">
                <div className="sticky top-0 flex h-8 items-center gap-2 border-b border-vsc-border bg-vsc-surface px-2">
                  <span className="font-semibold text-vsc-text">Sandwich</span>
                  <select
                    aria-label="Sandwich function"
                    className="ml-auto max-w-36 bg-vsc-input-bg text-vsc-text"
                    onChange={(event) =>
                      void loadSandwich(Number(event.target.value))
                    }
                    value={selectedFunctionId ?? ''}
                  >
                    {functionOptions.map((functionId) => (
                      <option key={functionId} value={functionId}>
                        fn #{functionId}
                      </option>
                    ))}
                  </select>
                </div>
                {sandwich.map((row, index) => (
                  <div
                    className="flex gap-2 border-b border-vsc-border-subtle px-2 py-1 text-vsc-text-muted"
                    key={`${row.direction}-${row.depth}-${row.functionId}-${index}`}
                  >
                    <span>
                      {row.direction === 1
                        ? 'caller'
                        : row.direction === 3
                          ? 'callee'
                          : 'selected'}
                    </span>
                    <span style={{ paddingLeft: row.depth * 6 }}>
                      fn #{row.functionId}
                    </span>
                    <span className="ml-auto">{formatNs(row.totalNs)}</span>
                  </div>
                ))}
              </section>
              <section className="min-w-0 overflow-auto">
                <div className="sticky top-0 z-10 flex h-8 items-center gap-1 border-b border-vsc-border bg-vsc-surface px-2">
                  <span className="font-semibold text-vsc-text">
                    Value inspector
                  </span>
                  <span
                    className="ml-auto max-w-24 truncate text-[10px] text-vsc-text-faint"
                    title={valueDiffLeft?.label}
                  >
                    L: {valueDiffLeft?.label ?? '—'}
                  </span>
                  <span
                    className="max-w-24 truncate text-[10px] text-vsc-text-faint"
                    title={valueDiffRight?.label}
                  >
                    R: {valueDiffRight?.label ?? '—'}
                  </span>
                  <button
                    className="h-6 rounded border border-vsc-border px-1.5 text-vsc-text hover:bg-vsc-list-hover disabled:opacity-50"
                    disabled={
                      valueDiffLeft == null ||
                      valueDiffRight == null ||
                      loadingAdvanced
                    }
                    onClick={() => void compareValues()}
                    type="button"
                  >
                    vdiff
                  </button>
                </div>
                {valueRefs.length === 0 ? (
                  <div className="p-3 text-vsc-text-faint">
                    No captured values
                  </div>
                ) : (
                  valueRefs.map((value) => (
                    <div
                      className="border-b border-vsc-border-subtle px-2 py-1"
                      key={value.id}
                      title={value.diagnostic ?? value.rootCid ?? undefined}
                    >
                      <div className="flex gap-2">
                        <span className="font-semibold text-vsc-text">
                          {value.role}
                        </span>
                        <span className="text-vsc-text-faint">
                          {valueAvailability(value.availability)}
                        </span>
                        {value.promotionTrigger != null && (
                          <span className="text-vsc-yellow">promoted</span>
                        )}
                        {value.rootCid != null && selectedId != null && (
                          <span className="ml-auto flex gap-1">
                            <button
                              className="border-0 bg-transparent p-0 text-vsc-accent hover:underline"
                              onClick={() => {
                                const boundaryId = selectedId;
                                const cid = value.rootCid;
                                if (boundaryId == null || cid == null) return;
                                void inspectValue({
                                  boundaryId,
                                  cid,
                                  label: `${value.role}:${value.id}`,
                                });
                              }}
                              type="button"
                            >
                              inspect
                            </button>
                            <button
                              aria-label={`Use ${value.role} as left value`}
                              className="border-0 bg-transparent p-0 text-vsc-text-muted hover:text-vsc-text"
                              onClick={() => {
                                const boundaryId = selectedId;
                                const cid = value.rootCid;
                                if (boundaryId == null || cid == null) return;
                                setValueDiffLeft({
                                  boundaryId,
                                  cid,
                                  label: `${value.role}:${shortCid(cid)}`,
                                });
                              }}
                              type="button"
                            >
                              L
                            </button>
                            <button
                              aria-label={`Use ${value.role} as right value`}
                              className="border-0 bg-transparent p-0 text-vsc-text-muted hover:text-vsc-text"
                              onClick={() => {
                                const boundaryId = selectedId;
                                const cid = value.rootCid;
                                if (boundaryId == null || cid == null) return;
                                setValueDiffRight({
                                  boundaryId,
                                  cid,
                                  label: `${value.role}:${shortCid(cid)}`,
                                });
                              }}
                              type="button"
                            >
                              R
                            </button>
                          </span>
                        )}
                      </div>
                      <div className="truncate text-[10px] text-vsc-text-muted">
                        {value.rootCid ?? value.id}
                      </div>
                    </div>
                  ))
                )}
                {valueRows.length > 0 && (
                  <div className="border-t border-vsc-border">
                    <div className="flex items-center gap-2 bg-vsc-bg px-2 py-1 text-[10px] text-vsc-text-muted">
                      <span>
                        {valueRowsAreDiff
                          ? 'Merkle diff'
                          : `DAG · ${valueInspection?.label ?? 'value'}`}
                      </span>
                      <span className="ml-auto">{valueRows.length} rows</span>
                      {valueRowsTruncated && (
                        <span className="text-vsc-yellow">
                          bounded · descend to continue
                        </span>
                      )}
                    </div>
                    {valueRows.map((row, index) => {
                      const navigableCid =
                        !valueRowsAreDiff && row.kind === 2
                          ? row.secondaryCid
                          : !valueRowsAreDiff && row.kind === 3
                            ? row.primaryCid
                            : null;
                      const blobBoundary =
                        row.kind === 6
                          ? valueDiffRight?.boundaryId
                          : valueRowsAreDiff
                            ? valueDiffLeft?.boundaryId
                            : valueInspection?.boundaryId;
                      const blobCid =
                        row.kind === 2 || row.kind === 5 || row.kind === 6
                          ? row.secondaryCid
                          : row.primaryCid;
                      return (
                        <div
                          className="flex items-center gap-1 border-t border-vsc-border-subtle px-2 py-1 text-[10px]"
                          key={`${row.kind}-${row.primaryCid}-${row.secondaryCid}-${row.ordinal}-${index}`}
                          style={{
                            paddingLeft: 8 + Math.min(row.depth, 16) * 7,
                          }}
                        >
                          <span className="w-16 shrink-0 text-vsc-text-faint">
                            {valueRowKind(row.kind)}
                          </span>
                          <span
                            className="min-w-0 flex-1 truncate text-vsc-text-muted"
                            title={`${row.primaryCid ?? ''} ${row.secondaryCid ?? ''}`}
                          >
                            {shortCid(row.primaryCid)}
                            {row.secondaryCid != null
                              ? ` → ${shortCid(row.secondaryCid)}`
                              : ''}
                          </span>
                          {row.equal && (
                            <span className="text-vsc-green">equal</span>
                          )}
                          {row.logicalLength != null && (
                            <span className="text-vsc-text-faint">
                              {row.logicalLength.toString()} B
                            </span>
                          )}
                          {navigableCid != null && valueInspection != null && (
                            <button
                              className="border-0 bg-transparent p-0 text-vsc-accent hover:underline"
                              onClick={() =>
                                void inspectValue({
                                  ...valueInspection,
                                  cid: navigableCid,
                                  label: shortCid(navigableCid),
                                })
                              }
                              type="button"
                            >
                              descend
                            </button>
                          )}
                          {blobBoundary != null && blobCid != null && (
                            <a
                              className="text-vsc-accent hover:underline"
                              href={valueBlobUrl(blobBoundary, blobCid)}
                              rel="noreferrer"
                              target="_blank"
                            >
                              bytes
                            </a>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </section>
            </div>
          </section>
        </div>
      )}
    </div>
  );
};

const LeftHeavyCanvas: FC<{ rows: LeftHeavyRow[] }> = ({ rows }) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    const canvas = canvasRef.current;
    if (container == null || canvas == null) return;
    const render = () => drawLeftHeavy(canvas, container, rows);
    render();
    const observer = new ResizeObserver(render);
    observer.observe(container);
    return () => observer.disconnect();
  }, [rows]);

  const height = Math.max(1, rows.length) * 19 + 8;
  return (
    <div className="min-h-0 flex-1 overflow-auto" ref={containerRef}>
      <canvas
        aria-label="Left Heavy aggregate calling-context view"
        className="block"
        ref={canvasRef}
        style={{ height, width: '100%' }}
      />
    </div>
  );
};

function drawLeftHeavy(
  canvas: HTMLCanvasElement,
  container: HTMLDivElement,
  rows: LeftHeavyRow[],
): void {
  const ratio = window.devicePixelRatio || 1;
  const width = Math.max(1, container.clientWidth);
  const height = Math.max(1, rows.length) * 19 + 8;
  canvas.width = Math.floor(width * ratio);
  canvas.height = Math.floor(height * ratio);
  const context = canvas.getContext('2d');
  if (context == null) return;
  context.scale(ratio, ratio);
  context.clearRect(0, 0, width, height);
  context.font = '11px ui-monospace, SFMono-Regular, Menlo, monospace';
  context.textBaseline = 'middle';
  rows.forEach((row, index) => {
    const y = 4 + index * 19;
    const indent = Math.min(row.depth, 32) * 11;
    const available = Math.max(1, width - indent - 8);
    // One device pixel is the minimum visible width; there is no percentage
    // floor that can inflate a tiny context into a misleading block.
    const extent = Math.max(
      1 / ratio,
      (available * Math.min(row.extentPpm, 1_000_000)) / 1_000_000,
    );
    context.fillStyle = row.syntheticSmaller
      ? 'rgba(128,128,128,0.30)'
      : colorForFunction(row.functionId);
    context.fillRect(indent + 4, y, extent, 16);
    context.strokeStyle = 'rgba(255,255,255,0.15)';
    context.strokeRect(indent + 4, y, extent, 16);
    context.fillStyle = '#d4d4d4';
    const label = row.syntheticSmaller
      ? `smaller (${row.calls.toString()} calls)`
      : `fn #${row.functionId} · ${row.calls.toString()} calls · ${formatNs(
          row.totalNs,
        )}`;
    context.save();
    context.beginPath();
    context.rect(indent + 7, y, Math.max(0, extent - 5), 16);
    context.clip();
    context.fillText(label, indent + 8, y + 8);
    context.restore();
  });
}

function colorForFunction(functionId: number): string {
  const hue = (functionId * 137.508) % 360;
  return `hsla(${hue}, 55%, 44%, 0.86)`;
}

function runStateColor(state: number): string {
  if (state === 5) return 'bg-vsc-green';
  if (state === 4 || state === 6) return 'bg-vsc-red';
  if (state === 3) return 'bg-vsc-accent';
  return 'bg-vsc-yellow';
}

function formatRunTime(createdMs: number): string {
  if (!Number.isFinite(createdMs) || createdMs === 0) return 'unknown';
  return new Date(createdMs).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

function formatNs(value: bigint): string {
  if (value >= 1_000_000_000n) return `${Number(value / 1_000_000n) / 1000}s`;
  if (value >= 1_000_000n) return `${Number(value / 1_000n) / 1000}ms`;
  if (value >= 1_000n) return `${Number(value) / 1000}µs`;
  return `${value.toString()}ns`;
}

function formatSignedNs(value: bigint): string {
  const prefix = value > 0n ? '+' : value < 0n ? '−' : '';
  return `${prefix}${formatNs(value < 0n ? -value : value)}`;
}

function valueAvailability(value: number): string {
  return (
    [
      'unknown',
      'pending',
      'available',
      'missing',
      'omitted',
      'lost',
      'promoted',
    ][value] ?? 'unknown'
  );
}

function valueRowKind(kind: ObserveValueDagRow['kind']): string {
  return (
    {
      1: 'node',
      2: 'child',
      3: 'resume',
      4: 'diff',
      5: 'left child',
      6: 'right child',
      7: 'resume diff',
    } as const
  )[kind];
}

function shortCid(cid: string | null): string {
  if (cid == null) return '∅';
  return cid.length <= 14 ? cid : `${cid.slice(0, 8)}…${cid.slice(-5)}`;
}

function valueBlobUrl(boundaryId: string, cid: string): string {
  return `/api/obs/blob/${encodeURIComponent(boundaryId)}/${encodeURIComponent(cid)}`;
}

function errorMessage(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}
