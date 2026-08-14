"use client";

import { useEffect } from "react";
import { useSearchParams } from "next/navigation";

// Small client component that reads ?call=N from the URL and:
//   1. Opens the matching <details id="call-N"> block
//   2. Opens its parent transcript section if needed
//   3. Scrolls it into view
//
// The server component renders all turn blocks closed by default (Slack canvas
// links land on this page when a user wants to inspect one specific failure;
// expanding everything else would just add noise).
/**
 * Client component that reads ?call=N from the URL and, on mount, opens the matching
 * transcript <details id="call-N"> block (and its assistant-text sub-block) then scrolls
 * it into view, so evidence/Slack deep links land on the relevant call. Renders nothing.
 * @returns null (side-effect only)
 */
export default function CallScroller() {
  const params = useSearchParams();

  useEffect(() => {
    const callStr = params.get("call");
    if (callStr == null) return;
    const callNum = Number(callStr);
    if (!Number.isFinite(callNum)) return;

    // Wait one frame so React has the <details> elements mounted.
    const raf = requestAnimationFrame(() => {
      const el = document.getElementById(`call-${callNum}`);
      if (!el || !(el instanceof HTMLDetailsElement)) return;
      el.open = true;
      // If the assistant-text sub-detail exists, open it so the user lands on
      // the model's words rather than tool plumbing.
      const text = el.querySelector("details.run-subblock[open]");
      if (text instanceof HTMLDetailsElement) {
        text.open = true;
      }
      el.scrollIntoView({ behavior: "smooth", block: "start" });
    });
    return () => cancelAnimationFrame(raf);
  }, [params]);

  return null;
}
