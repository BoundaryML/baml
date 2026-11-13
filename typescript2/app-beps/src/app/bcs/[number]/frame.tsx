"use client";

// The sandboxed frame a proposal page renders in, and what the site and the page say to each other
// (see BRIDGE_SCRIPT in src/lib/bcs-render.ts): the page asks to open another of the proposal's
// pages, to comment on a block, or to show a block's threads; the site sends the page how many
// open threads each block has, and asks it to scroll to a block or to the address's #fragment.
import { forwardRef, useCallback, useEffect, useImperativeHandle, useRef } from "react";
import { useRouter } from "next/navigation";

/**
 * Without allow-same-origin the page's scripts run in an opaque origin: they can draw its diagrams
 * but not read this site, its cookies or its storage, nor navigate it.
 */
const FRAME_SANDBOX = "allow-scripts allow-popups allow-popups-to-escape-sandbox";

export type BcsFrameHandle = {
  focus: (block: string) => void;
  /** Moves keyboard focus into the page to pick a block to comment on. */
  pick: () => void;
};

type Props = {
  title: string;
  document: string;
  number: number;
  /** Open threads per block, shown as badges in the page. */
  counts: Record<string, number>;
  onComment: (block: string, text: string) => void;
  onShowThread: (block: string) => void;
  /** The page's bridge is listening: anything sent before it loaded was lost. */
  onReady: () => void;
};

export const BcsFrame = forwardRef<BcsFrameHandle, Props>(function BcsFrame(
  { title, document, number, counts, onComment, onShowThread, onReady },
  handle
) {
  const frame = useRef<HTMLIFrameElement>(null);
  const router = useRouter();
  const post = useCallback((data: unknown) => frame.current?.contentWindow?.postMessage(data, "*"), []);
  const latest = useRef({ counts, onComment, onShowThread, onReady });
  latest.current = { counts, onComment, onShowThread, onReady };

  useImperativeHandle(
    handle,
    () => ({
      focus: (block) => post({ type: "bcs-focus", block }),
      pick: () => {
        frame.current?.focus();
        post({ type: "bcs-pick" });
      },
    }),
    [post]
  );

  useEffect(() => {
    post({ type: "bcs-marks", counts });
  }, [counts, post]);

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (event.source !== frame.current?.contentWindow) return;
      const data = event.data as { type?: unknown; href?: unknown; block?: unknown; text?: unknown } | null;
      if (!data) return;
      if (data.type === "bcs-ready") {
        post({ type: "bcs-marks", counts: latest.current.counts });
        latest.current.onReady();
      } else if (data.type === "bcs-open-page" && typeof data.href === "string") {
        // Only this proposal's pages: anything else the page sends is ignored.
        const target = new URL(data.href, window.location.origin);
        if (target.origin !== window.location.origin || target.pathname !== `/bcs/${number}`) return;
        router.push(target.pathname + target.search + target.hash);
        // The same page and version: the frame does not reload, so scroll it now.
        if (target.search === window.location.search && target.hash) {
          post({ type: "bcs-scroll", id: decodeURIComponent(target.hash.slice(1)) });
        }
      } else if (data.type === "bcs-comment" && typeof data.block === "string") {
        latest.current.onComment(data.block.slice(0, 100), typeof data.text === "string" ? data.text.slice(0, 300) : "");
      } else if (data.type === "bcs-show-thread" && typeof data.block === "string") {
        latest.current.onShowThread(data.block);
      }
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [number, post, router]);

  // Every page, HTML ones included, scrolls to the address's #fragment once it loads, and again
  // when it changes (the table of contents links to #fragments).
  const scrollToHash = useCallback(() => {
    const id = decodeURIComponent(window.location.hash.slice(1));
    if (id) post({ type: "bcs-scroll", id });
  }, [post]);
  useEffect(() => {
    window.addEventListener("hashchange", scrollToHash);
    return () => window.removeEventListener("hashchange", scrollToHash);
  }, [scrollToHash]);

  return (
    <iframe
      ref={frame}
      title={title}
      sandbox={FRAME_SANDBOX}
      srcDoc={document}
      onLoad={scrollToHash}
      className="flex-1 min-w-0 w-full border-0 bg-white"
    />
  );
});
