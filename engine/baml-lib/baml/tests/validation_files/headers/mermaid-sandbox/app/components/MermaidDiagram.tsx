"use client";

import mermaid from "mermaid";
import { useEffect, useRef, useState } from "react";

type MermaidDiagramProps = {
  chart: string;
  className?: string;
};

let initialized = false;

const escapeHtml = (input: string) =>
  input
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");

const sanitizeSvgForError = (error: unknown) => {
  if (error instanceof Error) {
    return escapeHtml(error.message);
  }
  return escapeHtml(String(error));
};

export default function MermaidDiagram({ chart, className }: MermaidDiagramProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [svg, setSvg] = useState<string>("");
  const idRef = useRef<string>();

  if (!idRef.current) {
    idRef.current = `mermaid-${Math.random().toString(36).slice(2, 11)}`;
  }

  useEffect(() => {
    if (!initialized) {
      mermaid.initialize({ startOnLoad: false });
      initialized = true;
    }

    let cancelled = false;

    const render = async () => {
      try {
        const { svg: renderedSvg } = await mermaid.render(idRef.current!, chart);
        if (!cancelled) {
          setSvg(renderedSvg);
        }
      } catch (error) {
        if (!cancelled) {
          const safeError = sanitizeSvgForError(error);
          setSvg(`<pre>${safeError}</pre>`);
        }
        console.error("Failed to render mermaid diagram", error);
      }
    };

    render();

    return () => {
      cancelled = true;
      setSvg("");
      if (containerRef.current) {
        containerRef.current.innerHTML = "";
      }
    };
  }, [chart]);

  return (
    <div
      ref={containerRef}
      className={className}
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
