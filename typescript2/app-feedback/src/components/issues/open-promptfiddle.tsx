"use client";
import { useState } from "react";

/** PromptFiddle currently accepts editor input, not URL imports. Never fake a share URL. */
export function OpenPromptFiddle({ files }: { files: Record<string, string> }) {
  const [message, setMessage] = useState("");
  async function copy() {
    const code = Object.entries(files).filter(([name]) => name.endsWith(".baml"))
      .map(([name, contents]) => `// ${name}\n${contents}`).join("\n\n");
    try {
      await navigator.clipboard.writeText(code);
      setMessage("Copied. Paste into the PromptFiddle editor. Its BAML version may differ from this repro.");
    } catch { setMessage("Copy the code below, then paste it into PromptFiddle."); }
  }
  return <div className="border-t px-3 py-2 text-xs">
    <a href="https://www.promptfiddle.com/" target="_blank" rel="noopener noreferrer" className="underline" onClick={() => { void copy(); }}>Open in PromptFiddle</a>
    <span className="ml-2 text-muted-foreground">Copy and paste; automatic import unavailable.</span>
    {message && <p role="status" className="mt-2">{message}</p>}
  </div>;
}
