"use client";

import { useState } from "react";

export function InvestigationPrompt({ prompt }: { prompt: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  async function copy() {
    try {
      await navigator.clipboard.writeText(prompt);
      setState("copied");
    } catch {
      setState("failed");
    }
  }
  return <section className="rounded-lg border bg-card p-4 space-y-3">
    <h2 className="font-medium text-sm">Prompt for your agent to investigate</h2>
    <p className="text-xs text-muted-foreground">Includes the BAML version setup, repro files, command and expected behavior.</p>
    <button type="button" onClick={copy} className="rounded-md border px-3 py-1.5 text-sm hover:bg-muted">{state === "copied" ? "Copied" : "Copy prompt"}</button>
    <span role="status" className="block text-xs text-muted-foreground">{state === "failed" ? "Copy unavailable. Select and copy the prompt below." : state === "copied" ? "Prompt copied to clipboard." : ""}</span>
    <details><summary className="cursor-pointer text-xs">View prompt</summary>
      <textarea aria-label="Investigation prompt" readOnly value={prompt} className="mt-2 w-full min-h-64 rounded border p-2 text-xs font-mono" />
    </details>
  </section>;
}
