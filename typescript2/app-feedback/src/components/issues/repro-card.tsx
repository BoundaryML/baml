import { OpenPromptFiddle } from "./open-promptfiddle";
import { CodeBlock, CodeFiles } from "@/components/code";
import type { Repro } from "@/lib/types";

function expectation(repro: Repro): string {
  switch (repro.expectation.check) {
    case "should_compile": return "Expected: source compiles";
    case "should_not_compile": return "Expected: source is rejected";
    case "should_evaluate_to":
      return `Expected: ${JSON.stringify(repro.expectation.expected)}`;
    case "requires_inspection": return repro.verification === "python_unittest" ? "Expected: executable assertions pass" : "Needs manual inspection";
  }
}

function Files({ files }: { files: Record<string, string> }) {
  const entries = Object.entries(files).filter(([name]) => name !== "baml.toml");
  return <CodeFiles files={entries} />;
}

function Output({ title, output }: { title: string; output?: string | null }) {
  return (
    <div className="border-t">
      <div className="px-3 py-2 text-xs font-medium">{title}</div>
      {output ? <CodeBlock text={output} language="text" /> : (
        <p className="px-3 pb-3 text-sm text-muted-foreground">No execution output recorded.</p>
      )}
    </div>
  );
}

export function ReproCard({ repro, comparison = false }: { repro: Repro; comparison?: boolean }) {
  const source = repro.source_files && Object.keys(repro.source_files).length > 0
    ? repro.source_files : null;
  const result = repro.result === "passes" ? "Passes"
    : repro.result === "fails" ? "Fails"
    : repro.result === "invalid_repro" ? "Invalid reproduction"
    : repro.result === "unsupported_verification" ? "Verification unavailable"
    : repro.result === "verification_error" ? "Verification failed to run"
    : repro.result === "inconclusive" ? "Inconclusive" : "Result not classified";
  return (
    <div className="overflow-hidden rounded-md border">
      <div className="flex flex-wrap items-center justify-between gap-2 bg-muted/60 px-3 py-2 text-xs">
        <span className="font-medium">{comparison ? "Comparison" : "Reproduction"} · {result}</span>
        <span>{expectation(repro)}</span>
      </div>
      <div className="px-3 py-2 text-xs text-muted-foreground">
        <span className="font-mono">$ {source ? "baml check" : repro.command}</span>
        {repro.verified_version && <span> · BAML {repro.verified_version}</span>}
      </div>
      {repro.setup && <p className="px-3 pb-2 text-sm">Setup: {repro.setup}</p>}
      <Files files={source ?? repro.files} />
      <OpenPromptFiddle files={source ?? repro.files} />
      <details className="border-t">
        <summary className="cursor-pointer px-3 py-2 text-sm">Execution output (stdout / stderr)</summary>
        <Output title={source ? "Compiler output (stdout / stderr)" : "Actual output (stdout / stderr)"}
          output={source ? repro.source_observed : repro.observed} />
      </details>
      {source && (
        <details className="border-t">
          <summary className="cursor-pointer px-3 py-2 text-sm">Full regression test and result</summary>
          <div className="px-3 py-2 font-mono text-xs">$ {repro.command}</div>
          <Files files={repro.files} />
          <Output title="Regression test output (stdout / stderr)" output={repro.observed} />
        </details>
      )}
    </div>
  );
}
