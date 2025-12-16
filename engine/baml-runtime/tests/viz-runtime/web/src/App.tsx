import { useEffect, useMemo, useState } from "react";
import { parseAllDocuments } from "yaml";
import type { CombinedRow, SnapshotEntry, SnapshotRow } from "./types";

const snapshotModules = import.meta.glob("../../snapshots/**/*.snap", {
  eager: true,
  query: "?raw",
  import: "default",
});

const sourceModules = import.meta.glob("../../testdata/*.baml", {
  eager: true,
  query: "?raw",
  import: "default",
});

function parseSnapshotFile(path: string, raw: string | undefined): SnapshotEntry | null {
  if (typeof raw !== "string") return null;

  const docs = parseAllDocuments(raw);
  const payload = docs.at(-1)?.toJSON() as
    | { fixture?: string; snapshots?: unknown }
    | undefined;
  const meta = docs.at(0)?.toJSON() as { input_file?: unknown } | undefined;
  if (!payload || !Array.isArray(payload.snapshots)) return null;

  const fileName = path.split("/").pop() ?? "snapshot";
  const fixture = payload.fixture ?? fileName;

  const inputFile =
    typeof meta?.input_file === "string" ? meta.input_file.split("/").pop() : undefined;

  return { fixture, inputFile, rows: payload.snapshots as SnapshotRow[] };
}

function discoverSnapshots(): SnapshotEntry[] {
  return Object.entries(snapshotModules)
    .filter(([path]) => path.endsWith(".snap"))
    .map(([path, raw]) => parseSnapshotFile(path, raw as string | undefined))
    .filter((entry): entry is SnapshotEntry => Boolean(entry))
    .sort((a, b) => a.fixture.localeCompare(b.fixture));
}

function toLabel(entry: SnapshotEntry): string {
  return entry.fixture;
}

export default function App() {
  const [manifest, setManifest] = useState<SnapshotEntry[]>([]);
  const [selected, setSelected] = useState<string>();
  const [rows, setRows] = useState<CombinedRow[]>([]);
  const [source, setSource] = useState<{ name: string; content: string }>();

  const sourceMap = useMemo(() => {
    return Object.entries(sourceModules).reduce<Record<string, string>>((acc, [path, raw]) => {
      if (typeof raw === "string") {
        const name = path.split("/").pop();
        if (name) {
          acc[name] = raw as string;
        }
      }
      return acc;
    }, {});
  }, []);

  const current = useMemo(
    () => manifest.find((item) => item.fixture === selected),
    [manifest, selected],
  );

  useEffect(() => {
    const entries = discoverSnapshots();
    setManifest(entries);
    if (entries.length > 0) {
      setSelected(entries[0].fixture);
    }
  }, []);

  useEffect(() => {
    if (!current) {
      setRows([]);
      return;
    }

    const combined: CombinedRow[] = current.rows.map((row, idx) => {
      const state = row.state ?? { nodes: {}, frames: [] };
      return {
        index: idx,
        watchEvent: row.watch_event,
        stackAfter: Array.isArray(row.stack_after) ? row.stack_after : [],
        emittedEvents: Array.isArray(row.emitted_events) ? row.emitted_events : [],
      };
    });

    setRows(combined);
  }, [current]);

  useEffect(() => {
    if (!current) {
      setSource(undefined);
      return;
    }
    const targetName = current.inputFile ?? current.fixture;
    const content = sourceMap[targetName];
    if (content) {
      setSource({ name: targetName, content });
    } else {
      setSource(undefined);
    }
  }, [current, sourceMap]);

  return (
    <div style={styles.page}>
      <header style={styles.header}>
        <h1>Viz Runtime Snapshots</h1>
        <p>Watching {manifest.length} fixture(s)</p>
      </header>
      <section style={styles.sidebar}>
        <h2>Fixtures</h2>
        <div style={styles.list}>
          {manifest.map((entry) => (
            <button
              key={entry.fixture}
              onClick={() => setSelected(entry.fixture)}
              style={{
                ...styles.listItem,
                ...(selected === entry.fixture ? styles.listItemActive : {}),
              }}
            >
              {toLabel(entry)}
            </button>
          ))}
        </div>
      </section>
      <section style={styles.source}>
        <h2>Source</h2>
        {!source ? (
          <p style={styles.subtext}>Source not found for this fixture.</p>
        ) : (
          <>
            <div style={styles.subtext}>{source.name}</div>
            <pre style={{ ...styles.pre, height: "calc(100vh - 180px)", overflowY: "auto" }}>
              {source.content}
            </pre>
          </>
        )}
      </section>
      <main style={styles.main}>
        {!current ? (
          <p>No snapshots found yet. Run the Rust snapshot tests to generate them.</p>
        ) : (
          <table style={styles.table}>
            <thead>
              <tr>
                <th style={styles.th}>
                  <div>#</div>
                </th>
                <th style={styles.th}>
                  <div>Event</div>
                  <div style={styles.subtext}>Raw WatchNotification emitted by baml-runtime to watch_handler</div>
                </th>
                <th style={styles.th}>
                  <div>Stack</div>
                  <div style={styles.subtext}>VizStateReducer internal state after applying the event</div>
                </th>
                <th style={styles.th}>
                  <div>Emitted</div>
                  <div style={styles.subtext}>StateUpdate entries emitted by VizStateReducer</div>
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.index} style={styles.tr}>
                  <td style={styles.td}>{row.index}</td>
                  <td style={styles.td}>
                    <div>{row.watchEvent.kind}</div>
                    <div style={styles.subtext}>{row.watchEvent.function}</div>
                    <pre style={styles.pre}>
                      {JSON.stringify(row.watchEvent, null, 2)}
                    </pre>
                  </td>
                  <td style={styles.td}>
                    <pre style={styles.pre}>
                      {JSON.stringify(row.stackAfter, null, 2)}
                    </pre>
                  </td>
                  <td style={styles.td}>
                    {row.emittedEvents.length > 0 ? (
                      <pre style={styles.pre}>
                        {JSON.stringify(row.emittedEvents, null, 2)}
                      </pre>
                    ) : (
                      <div style={styles.subtext}>No emitted events</div>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </main>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  page: {
    display: "grid",
    gridTemplateColumns: "280px 380px 1fr",
    gridTemplateRows: "auto 1fr",
    height: "100vh",
    fontFamily: "Inter, system-ui, sans-serif",
  },
  header: {
    gridColumn: "1 / span 3",
    padding: "16px 20px",
    borderBottom: "1px solid #e6e6e6",
    background: "#fafafa",
  },
  sidebar: {
    borderRight: "1px solid #e6e6e6",
    padding: "12px",
    overflowY: "auto",
  },
  source: {
    borderRight: "1px solid #e6e6e6",
    padding: "12px",
    overflowY: "auto",
  },
  list: {
    display: "flex",
    flexDirection: "column",
    gap: "8px",
  },
  listItem: {
    textAlign: "left",
    padding: "8px 10px",
    borderRadius: "8px",
    border: "1px solid #d8d8d8",
    background: "#fff",
    cursor: "pointer",
  },
  listItemActive: {
    borderColor: "#6366f1",
    boxShadow: "0 0 0 2px rgba(99, 102, 241, 0.2)",
  },
  main: {
    overflow: "auto",
  },
  table: {
    width: "100%",
    borderCollapse: "collapse",
  },
  th: {
    textAlign: "left",
    padding: "8px 10px",
    borderBottom: "1px solid #e6e6e6",
    background: "#f7f7f7",
    position: "sticky",
    top: 0,
    zIndex: 1,
  },
  td: {
    padding: "8px 10px",
    verticalAlign: "top",
    borderBottom: "1px solid #f0f0f0",
  },
  tr: {
    background: "#fff",
  },
  pre: {
    margin: "0 0 6px 0",
    background: "#f8f8f8",
    padding: "6px",
    borderRadius: "6px",
    fontSize: "11px",
    overflowX: "auto",
  },
  badge: {
    display: "inline-block",
    padding: "2px 6px",
    borderRadius: "6px",
    background: "#eef2ff",
    color: "#3730a3",
    fontSize: "12px",
    marginTop: "4px",
  },
  subtext: {
    color: "#6b7280",
    fontSize: "12px",
  },
};
