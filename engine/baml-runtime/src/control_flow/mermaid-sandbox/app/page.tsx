import path from "node:path";
import fs from "node:fs/promises";
import { parseAllDocuments } from "yaml";

import HeaderFileWatcher from "./components/HeaderFileWatcher";
import MermaidDiagram from "./components/MermaidDiagram";

type ExampleRow = {
  baseName: string;
  hir?: {
    source: string;
    contents: string;
  };
  baml?: {
    fileName: string;
    contents: string;
  };
  graph?: {
    source: string;
    json: unknown;
  };
  mermaid?: {
    source: string;
    contents: string;
  };
  flattenStages?: {
    label: string;
    graph?: {
      source: string;
      json: unknown;
    };
    mermaid?: {
      source: string;
      contents: string;
    };
  }[];
  error?: string;
};

const SANDBOX_ROOT = process.cwd();
const CONTROL_FLOW_DIR = path.resolve(SANDBOX_ROOT, "..");
const SNAPSHOT_DIR = path.join(CONTROL_FLOW_DIR, "snapshots");
const TESTDATA_DIR = path.join(CONTROL_FLOW_DIR, "testdata");
const SNAPSHOT_PREFIX = "baml_runtime__control_flow__tests__headers__";

async function loadExamples(): Promise<ExampleRow[]> {
  const snapshotFiles = (await fs.readdir(SNAPSHOT_DIR).catch(() => []))
    .filter((file) => file.startsWith(SNAPSHOT_PREFIX) && file.endsWith(".snap"))
    .sort();

  const cases = new Map<string, ExampleRow>();

  for (const snapshotFile of snapshotFiles) {
    const snapshotPath = path.join(SNAPSHOT_DIR, snapshotFile);
    const withoutPrefix = snapshotFile.replace(SNAPSHOT_PREFIX, "");
    const withoutSuffix = withoutPrefix.replace(/\.snap$/, "");
    const [snapshotId, sourceHint] = withoutSuffix.split("@");
    const caseName = snapshotId.replace(/^headers__/, "");

    let entry = cases.get(caseName);
    if (!entry) {
      entry = { baseName: caseName };
      cases.set(caseName, entry);
    }

    if (!entry.baml) {
      const candidates = Array.from(
        new Set(
          [sourceHint, `${caseName}.baml`]
            .filter((candidate): candidate is string => Boolean(candidate && candidate.length > 0)),
        ),
      );

      let foundPath: string | null = null;
      for (const candidate of candidates) {
        const candidatePath = path.join(TESTDATA_DIR, candidate);
        try {
          const stat = await fs.stat(candidatePath);
          if (stat.isFile()) {
            foundPath = candidatePath;
            break;
          }
        } catch {
          // Missing candidate; try the next option.
        }
      }

      if (foundPath) {
        const contents = await fs.readFile(foundPath, "utf8");
        entry.baml = {
          fileName: path.relative(CONTROL_FLOW_DIR, foundPath),
          contents,
        };
      }
    }

    const rawSnapshot = await fs.readFile(snapshotPath, "utf8");
    const yamlDocs = parseAllDocuments(rawSnapshot);
    const dataDoc = yamlDocs.at(-1)?.toJSON();

    if (!dataDoc || typeof dataDoc !== "object" || dataDoc === null) {
      entry.error ??= `Snapshot ${snapshotFile} is missing data.`;
      continue;
    }

    const errorValue = (dataDoc as { __error?: unknown }).__error;
    if (typeof errorValue === "string") {
      entry.error = errorValue;
      entry.graph = {
        source: snapshotFile,
        json: { __error: errorValue },
      };
      entry.mermaid = undefined;
      continue;
    }

    const mermaidValue = (dataDoc as { mermaid?: unknown }).mermaid;
    if (typeof mermaidValue === "string") {
      entry.mermaid = {
        source: snapshotFile,
        contents: mermaidValue,
      };
    }

    const flatteningValue = (dataDoc as { flattening?: unknown }).flattening;
    if (flatteningValue && typeof flatteningValue === "object") {
      const stages: Array<[string, string]> = [
        ["pass1_remove_implicit", "Pass 1 – Remove implicit"],
        ["pass2_hoist_branch_arms", "Pass 2 – Hoist branch arms"],
        ["pass3_flatten_scopes", "Pass 3 – Flatten arms & scopes"],
      ];

      for (const [stageKey, stageLabel] of stages) {
        const stageData = (flatteningValue as Record<string, unknown>)[stageKey];
        if (!stageData || typeof stageData !== "object") {
          continue;
        }
        const mermaidStage = (stageData as { mermaid?: unknown }).mermaid;
        const stageEntry = {
          label: stageLabel,
          graph: (() => {
            const jsonStage =
              (stageData as { json?: unknown }).json ?? (stageData as { expr?: unknown }).expr;
            if (typeof jsonStage === "undefined") {
              return undefined;
            }
            return {
              source: `${snapshotFile}::${String(stageKey)}::json`,
              json: jsonStage,
            };
          })(),
          mermaid:
            typeof mermaidStage === "string"
              ? {
                  source: `${snapshotFile}::${String(stageKey)}`,
                  contents: mermaidStage,
                }
              : undefined,
        };
        if (!entry.flattenStages) {
          entry.flattenStages = [];
        }
        entry.flattenStages.push(stageEntry);
      }
    }

    const hirValue = (dataDoc as { hir?: unknown }).hir;
    if (typeof hirValue === "string") {
      entry.hir = {
        source: snapshotFile,
        contents: hirValue,
      };
    }

    const exprValue = (dataDoc as { expr?: unknown }).expr;
    if (exprValue && typeof exprValue === "object") {
      entry.graph = {
        source: snapshotFile,
        json: exprValue,
      };
    }
  }

  return Array.from(cases.values()).sort((a, b) => a.baseName.localeCompare(b.baseName));
}

export default async function Home() {
  const examples = await loadExamples();

  return (
    <main className="min-h-screen bg-white text-black px-6 py-10">
      <HeaderFileWatcher />
      <h1 className="text-3xl font-semibold mb-6">Mermaid Headers Playground</h1>

      {examples.length === 0 ? (
        <p className="text-gray-600">No control-flow snapshots detected.</p>
      ) : (
        <div className="space-y-10">
          {examples.map((example) => (
            <article key={example.baseName} className="border border-gray-200 rounded-md shadow-sm overflow-hidden">
              <header className="border-b border-gray-200 bg-gray-50 px-4 py-3">
                <h2 className="text-xl font-medium">{example.baseName}</h2>
                <p className="mt-1 text-sm text-gray-600">
                  {example.baml?.fileName ?? "(no source)"}
                </p>
              </header>
              <div className="grid gap-6 lg:grid-cols-3 p-4">
                <section className="space-y-2">
                  <h3 className="font-semibold text-sm text-gray-700 uppercase tracking-wide">HIR</h3>
                  <pre className="overflow-auto rounded-md border border-gray-200 bg-white p-4 text-sm text-gray-800">
                    <code>
                      {example.hir
                        ? example.hir.contents
                        : "// No HIR snapshot available."}
                    </code>
                  </pre>
                </section>

                <section className="space-y-2">
                  <h3 className="font-semibold text-sm text-gray-700 uppercase tracking-wide">BAML Source</h3>
                  <pre className="overflow-auto rounded-md border border-gray-200 bg-white p-4 text-sm text-gray-800">
                    <code>
                      {example.baml
                        ? example.baml.contents
                        : "// No matching .baml file found for this diagram."}
                    </code>
                  </pre>
                </section>

                <section className="space-y-2">
                  <h3 className="font-semibold text-sm text-gray-700 uppercase tracking-wide">Graph Snapshot</h3>
                  <pre className="overflow-auto rounded-md border border-gray-200 bg-white p-4 text-sm text-gray-800">
                    <code>
                      {example.graph
                        ? JSON.stringify(example.graph.json, null, 2)
                        : "// No graph snapshot available."}
                    </code>
                  </pre>
                </section>
              </div>
              <div className="space-y-4 border-t border-gray-200 px-4 py-4">
                <h3 className="font-semibold text-sm text-gray-700 uppercase tracking-wide">Visualizations</h3>
                {example.error ? (
                  <p className="text-sm text-red-600">{example.error}</p>
                ) : (
                  <div className="space-y-6">
                    <section className="border border-gray-200 rounded-md overflow-hidden">
                      <header className="px-3 py-2 text-sm font-medium bg-gray-50 border-b border-gray-200">
                        Original CFG
                      </header>
                      <div className="p-4 bg-white">
                        {example.mermaid ? (
                          <MermaidDiagram chart={example.mermaid.contents} className="w-full overflow-auto" />
                        ) : (
                          <p className="text-sm text-gray-500">No diagram data.</p>
                        )}
                      </div>
                    </section>
                    {!!example.flattenStages?.length && (
                      <section className="space-y-4">
                        <h4 className="text-base font-semibold text-gray-800">Flattening Stages</h4>
                        {example.flattenStages.map((stage, index) => (
                          <article
                            key={`${example.baseName}-stage-${index}-${stage.label}`}
                            className="border border-gray-200 rounded-md overflow-hidden"
                          >
                            <header className="px-3 py-2 text-sm font-medium bg-gray-50 border-b border-gray-200">
                              {stage.label}
                            </header>
                            <div className="grid gap-4 md:grid-cols-2 p-4 bg-white">
                              <div className="space-y-2">
                                <h5 className="font-semibold text-xs text-gray-700 uppercase tracking-wide">JSON</h5>
                                <pre className="overflow-auto rounded-md border border-gray-200 bg-white p-3 text-xs text-gray-800">
                                  <code>
                                    {stage.graph
                                      ? JSON.stringify(stage.graph.json, null, 2)
                                      : "// No JSON snapshot available."}
                                  </code>
                                </pre>
                              </div>
                              <div className="space-y-2">
                                <h5 className="font-semibold text-xs text-gray-700 uppercase tracking-wide">Mermaid</h5>
                                {stage.mermaid ? (
                                  <MermaidDiagram chart={stage.mermaid.contents} className="w-full overflow-auto" />
                                ) : (
                                  <p className="text-sm text-gray-500">No diagram data.</p>
                                )}
                              </div>
                            </div>
                          </article>
                        ))}
                      </section>
                    )}
                  </div>
                )}
              </div>
            </article>
          ))}
        </div>
      )}
    </main>
  );
}
