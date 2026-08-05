"use client";

import { useEffect, useId, useState } from "react";
import { ShikiCodeBlock } from "@/components/ui/shiki-code-block";

interface MermaidDiagramProps {
  code: string;
}

function useIsDarkTheme(): boolean {
  const [isDark, setIsDark] = useState(false);

  useEffect(() => {
    const root = document.documentElement;
    const update = () => setIsDark(root.classList.contains("dark"));
    update();
    const observer = new MutationObserver(update);
    observer.observe(root, { attributes: true, attributeFilter: ["class"] });
    return () => observer.disconnect();
  }, []);

  return isDark;
}

export function MermaidDiagram({ code }: MermaidDiagramProps) {
  const id = useId();
  const isDark = useIsDarkTheme();
  const [svg, setSvg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function render() {
      try {
        const mermaid = (await import("mermaid")).default;
        mermaid.initialize({
          startOnLoad: false,
          theme: isDark ? "dark" : "default",
          securityLevel: "strict",
        });
        // Ids must be valid CSS selectors; useId's colons are not
        const renderId = `mermaid-${id.replace(/[^a-zA-Z0-9-]/g, "")}`;
        const { svg } = await mermaid.render(renderId, code);
        if (!cancelled) {
          setSvg(svg);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      }
    }

    render();
    return () => {
      cancelled = true;
    };
  }, [code, id, isDark]);

  // Fall back to a plain code block when the diagram fails to parse
  if (error) {
    return <ShikiCodeBlock code={code} language="text" />;
  }

  if (!svg) {
    return (
      <div className="my-5 flex min-h-24 items-center justify-center rounded-xl border border-border bg-code-bg p-5 text-sm text-muted-foreground">
        Rendering diagram…
      </div>
    );
  }

  return (
    <div
      className="my-5 flex justify-center overflow-x-auto rounded-xl border border-border bg-background p-5 [&_svg]:max-w-full [&_svg]:h-auto"
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
