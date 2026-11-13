"use client";

// The sandboxed frame a proposal page renders in, and the little the site and the page say to each
// other: the page asks to open another of the proposal's pages, and the site asks the page to
// scroll to the #fragment in the address.
import { useEffect, useRef } from "react";
import { useRouter } from "next/navigation";

/**
 * Without allow-same-origin the page's scripts run in an opaque origin: they can draw its diagrams
 * but not read this site, its cookies or its storage, nor navigate it.
 */
const FRAME_SANDBOX = "allow-scripts allow-popups allow-popups-to-escape-sandbox";

export function BcsFrame({ title, document, number }: { title: string; document: string; number: number }) {
  const frame = useRef<HTMLIFrameElement>(null);
  const router = useRouter();

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      if (event.source !== frame.current?.contentWindow) return;
      const data = event.data as { type?: unknown; href?: unknown } | null;
      if (data?.type !== "bcs-open-page" || typeof data.href !== "string") return;
      // Only this proposal's pages: anything else the page sends is ignored.
      const target = new URL(data.href, window.location.origin);
      if (target.origin !== window.location.origin || target.pathname !== `/bcs/${number}`) return;
      router.push(target.pathname + target.search + target.hash);
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, [number, router]);

  const scrollToHash = () => {
    const id = decodeURIComponent(window.location.hash.slice(1));
    if (id) frame.current?.contentWindow?.postMessage({ type: "bcs-scroll", id }, "*");
  };

  return (
    <iframe
      ref={frame}
      title={title}
      sandbox={FRAME_SANDBOX}
      srcDoc={document}
      onLoad={scrollToHash}
      className="flex-1 w-full border-0 bg-white"
    />
  );
}
