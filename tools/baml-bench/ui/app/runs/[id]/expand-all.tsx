"use client";

import { useState } from "react";

// Toggle that opens/closes every <details> in the transcript at once, including
// the nested thinking / assistant-text / tool sub-blocks. The server renders
// everything collapsed; this lets you blow the whole transcript open in one click.
/**
 * Client component button that opens or closes every <details> in the transcript
 * at once (including nested thinking/assistant-text/tool sub-blocks), toggling its
 * label between "expand all" and "collapse all".
 * @returns the toggle button
 */
export default function ExpandAll() {
  const [open, setOpen] = useState(false);

  const toggle = () => {
    const next = !open;
    const root = document.getElementById("transcript");
    root?.querySelectorAll("details").forEach((d) => {
      (d as HTMLDetailsElement).open = next;
    });
    setOpen(next);
  };

  return (
    <button
      className="linkbtn"
      onClick={toggle}
      style={{ fontSize: 13, fontWeight: 400 }}
    >
      {open ? "collapse all" : "expand all"}
    </button>
  );
}
