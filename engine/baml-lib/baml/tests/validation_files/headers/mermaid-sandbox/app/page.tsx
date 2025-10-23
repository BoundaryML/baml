import path from "node:path";
import fs from "node:fs/promises";

import HeaderFileWatcher from "./components/HeaderFileWatcher";
import MermaidDiagram from "./components/MermaidDiagram";

type HeaderExample = {
  baseName: string;
  mmdFileName: string;
  mmdContents: string;
  bamlFileName: string | null;
  bamlContents: string | null;
};

const headersDir = path.resolve(process.cwd(), "..");

async function loadHeaderExamples(): Promise<HeaderExample[]> {
  const entries = await fs.readdir(headersDir, { withFileTypes: true });
  const mmdFiles = entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".mmd"))
    .map((entry) => entry.name)
    .sort((a, b) => a.localeCompare(b));

  const examples: HeaderExample[] = [];

  for (const mmdFile of mmdFiles) {
    const baseName = mmdFile.replace(/\.mmd$/, "");
    const mmdPath = path.join(headersDir, mmdFile);
    const mmdContents = await fs.readFile(mmdPath, "utf8");

    const bamlFile = `${baseName}.baml`;
    const bamlPath = path.join(headersDir, bamlFile);

    let bamlContents: string | null = null;
    let bamlFileName: string | null = null;

    try {
      bamlContents = await fs.readFile(bamlPath, "utf8");
      bamlFileName = bamlFile;
    } catch (error: unknown) {
      const err = error as NodeJS.ErrnoException;
      if (err?.code !== "ENOENT") {
        throw error;
      }
    }

    examples.push({
      baseName,
      mmdFileName: mmdFile,
      mmdContents,
      bamlFileName,
      bamlContents,
    });
  }

  return examples;
}

export default async function Home() {
  const files = await loadHeaderExamples();

  return (
    <main className="min-h-screen bg-white text-black px-6 py-10">
      <HeaderFileWatcher />
      <h1 className="text-3xl font-semibold mb-6">Mermaid Headers Playground</h1>

      {files.length === 0 ? (
        <p className="text-gray-600">No Mermaid files found in the headers directory.</p>
      ) : (
        <div className="space-y-10">
          {files.map((file) => (
            <article key={file.mmdFileName} className="border border-gray-200 rounded-md shadow-sm">
              <header className="border-b border-gray-200 bg-gray-50 px-4 py-3">
                <h2 className="text-xl font-medium">{file.baseName}</h2>
                <p className="mt-1 text-sm text-gray-600">
                  {file.bamlFileName ?? "(missing .baml)"} · {file.mmdFileName}
                </p>
              </header>
              <div className="grid gap-0 sm:[grid-template-columns:1fr_2fr]">
                <pre className="overflow-auto p-4 text-sm bg-white border-b sm:border-b-0 sm:border-r border-gray-200">
                  <code>
                    {file.bamlContents ?? "// No matching .baml file found for this diagram."}
                  </code>
                </pre>
                <div className="p-4 flex items-center justify-center bg-white">
                  <MermaidDiagram chart={file.mmdContents} className="w-full overflow-auto" />
                </div>
              </div>
            </article>
          ))}
        </div>
      )}
    </main>
  );
}
