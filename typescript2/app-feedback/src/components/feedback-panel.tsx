"use client";

import { useEffect, useRef, useState, type MouseEvent, type ReactNode } from "react";
import { FormattedText } from "@/components/code";
import type { PublicFeedback } from "@/lib/db";

/** Preserve the issue underneath the report, including scroll and tab state. */
export function FeedbackPanel({ ids, children }: { ids: string[]; children: ReactNode }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [id, setId] = useState<string | null>(null);
  const [report, setReport] = useState<PublicFeedback | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    if (!id) return;
    const controller = new AbortController();
    setReport(null);
    setError(null);
    if (!dialog.current?.open) dialog.current?.showModal();
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    fetch(`/api/feedback/${encodeURIComponent(id)}`, { signal: controller.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(response.status === 404 ? "Report not found." : "Could not load this report.");
        return response.json() as Promise<PublicFeedback>;
      })
      .then(setReport)
      .catch((reason) => { if (!controller.signal.aborted) setError(reason instanceof Error ? reason.message : "Could not load this report."); });
    return () => { controller.abort(); document.body.style.overflow = previousOverflow; };
  }, [id, attempt]);

  function openReport(event: MouseEvent<HTMLDivElement>) {
    if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const link = (event.target as Element).closest("a[href]");
    if (!link || link.getAttribute("target") === "_blank") return;
    const url = new URL(link.getAttribute("href")!, window.location.href);
    if (![window.location.origin, "https://app-feedback-phi.vercel.app", "https://app-feedback-baml.vercel.app", "https://atb2-feedback.fly.dev"].includes(url.origin)) return;
    const match = /^\/feedback\/([A-Za-z0-9_-]{1,120})\/?$/.exec(url.pathname);
    if (!match || !ids.includes(match[1])) return;
    event.preventDefault();
    setReport(null);
    setError(null);
    setId(match[1]);
  }

  return <div onClickCapture={openReport}>
    {children}
    <dialog ref={dialog} aria-labelledby="feedback-panel-title" onClose={() => setId(null)}
      onClick={(event) => { if (event.target === event.currentTarget) dialog.current?.close(); }}
      className="fixed inset-y-0 left-auto right-0 m-0 h-dvh max-h-dvh w-full max-w-xl border-l bg-background p-0 text-foreground shadow-xl backdrop:bg-black/40">
      <div className="min-h-full p-6">
        <header className="mb-5 flex items-center justify-between gap-4 border-b pb-4">
          <h2 id="feedback-panel-title" className="font-semibold">Original feedback</h2>
          <button type="button" autoFocus onClick={() => dialog.current?.close()} className="rounded border px-3 py-1 text-sm hover:bg-muted">Close</button>
        </header>
        <div aria-live="polite">
          {!report && !error && <p className="text-sm text-muted-foreground">Loading report…</p>}
          {error && <div role="alert" className="space-y-3"><p>{error}</p><button type="button" className="underline" onClick={() => setAttempt((n) => n + 1)}>Try again</button></div>}
          {report && <article className="space-y-4">
            <p className="break-all font-mono text-xs text-muted-foreground">{report.id}</p>
            <h3 className="break-words text-xl font-semibold">{report.title}</h3>
            <p className="text-sm text-muted-foreground">{report.source} · {new Date(report.received_at).toLocaleString("en-US", { timeZone: "UTC" })} UTC{report.toolchain && ` · BAML ${report.toolchain}`}</p>
            <FormattedText text={report.body} />
          </article>}
        </div>
      </div>
    </dialog>
  </div>;
}
