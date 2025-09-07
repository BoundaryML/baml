import { useAtomValue } from 'jotai';
import { useEffect, useRef, useCallback } from 'react';
import mermaid from 'mermaid';
import { Button } from '@baml/ui/button';
import { ZoomIn, ZoomOut, Maximize2 } from 'lucide-react';
import svgPanZoom from 'svg-pan-zoom';
import { functionGraphAtom } from '../../../atoms-orch-graph';
import { vscode } from '../../../../vscode';

// === BAML Mermaid CSS Override (media-like styling) ===
// This CSS is injected into the generated Mermaid SVG so it overrides Mermaid's defaults.
// It aligns the graph visuals with the media panel styling (rounded corners, thicker borders,
// VS Code theme colors) and adds hover feedback for clickable nodes.
const MERMAID_CSS_OVERRIDE = `
  /* Container font + base text color */
  #bamlMermaidSvg {
    font-family: inherit !important; /* use playground default font */
    color: var(--vscode-foreground) !important;
    /* Diagram-specific border color (solid by default; never transparent) */
    --baml-diagram-border-color: var(--vscode-panel-border);
    background: transparent !important; /* ensure SVG background is transparent */
  }

  /* Labels and text should match editor foreground */
  #bamlMermaidSvg .label text,
  #bamlMermaidSvg .nodeLabel,
  #bamlMermaidSvg .cluster-label text,
  #bamlMermaidSvg .cluster-label span,
  #bamlMermaidSvg .edgeLabel,
  #bamlMermaidSvg span { 
    fill: var(--vscode-foreground) !important;
    color: var(--vscode-foreground) !important;
    font-family: inherit !important;
    font-size: 1em !important;
  }

  /* Ensure foreignObject HTML content also inherits playground font/size */
  #bamlMermaidSvg foreignObject,
  #bamlMermaidSvg foreignObject div,
  #bamlMermaidSvg foreignObject span,
  #bamlMermaidSvg foreignObject p {
    font-family: inherit !important;
    font-size: 1em !important;
    line-height: 1.4;
  }

  /* Prevent cluster titles from being clipped by the foreignObject box */
  #bamlMermaidSvg .cluster-label foreignObject,
  #bamlMermaidSvg .cluster-label div {
    overflow: visible !important;
    max-width: none !important;
  }

  /* Node shapes: use editor bg fill and panel border stroke, thicker borders, rounded joins */
  #bamlMermaidSvg .node rect,
  #bamlMermaidSvg .node circle,
  #bamlMermaidSvg .node ellipse,
  #bamlMermaidSvg .node polygon,
  #bamlMermaidSvg .node path {
    fill: var(--vscode-editor-background) !important;
    stroke: var(--baml-diagram-border-color) !important;
    stroke-width: 5.4px !important; /* 1.5x thicker */
    stroke-linejoin: round !important;
    transition: fill 150ms ease, stroke 150ms ease, stroke-width 150ms ease, filter 150ms ease;
  }

  /* Cluster containers: subtle sidebar background with stronger border */
  #bamlMermaidSvg .cluster rect {
    fill: var(--vscode-sideBar-background) !important;
    stroke: var(--baml-diagram-border-color) !important;
    stroke-width: 5.4px !important; /* 1.5x thicker */
  }

  /* Edge labels: badge-like background with border and rounding */
  #bamlMermaidSvg .edgeLabel rect {
    fill: var(--vscode-editor-background) !important;
    stroke: var(--baml-diagram-border-color) !important;
    stroke-width: 3.6px !important; /* 1.5x thicker */
    opacity: 1 !important;
    rx: 6; ry: 6;
  }

  /* Links/edges: round caps/joins and thicker strokes to match media borders */
  #bamlMermaidSvg .flowchart-link,
  #bamlMermaidSvg .edgePath .path {
    stroke: var(--baml-diagram-border-color) !important;
    stroke-width: 5.7px !important; /* 1.5x thicker */
    // stroke-linecap: round !important;
    // stroke-linejoin: round !important;
  }

  /* Normalize Mermaid thickness utility classes to our desired thickness */
  #bamlMermaidSvg .edge-thickness-normal { stroke-width: 5.7px !important; }
  #bamlMermaidSvg .edge-thickness-thick { stroke-width: 6.9px !important; }
  #bamlMermaidSvg .edge-thickness-invisible { stroke-width: 0 !important; fill: none !important; }

  /* Arrowheads/markers use the same border color */
  #bamlMermaidSvg .arrowheadPath,
  #bamlMermaidSvg .marker,
  #bamlMermaidSvg .marker.cross {
    fill: var(--baml-diagram-border-color) !important;
    stroke: var(--baml-diagram-border-color) !important;
  }

  /* Clickable node hover: subtle fill shift, stronger stroke, and glow */
  #bamlMermaidSvg g.node.clickable:hover rect,
  #bamlMermaidSvg g.node.clickable:hover circle,
  #bamlMermaidSvg g.node.clickable:hover ellipse,
  #bamlMermaidSvg g.node.clickable:hover polygon,
  #bamlMermaidSvg g.node.clickable:hover path {
    fill: var(--vscode-sideBar-background) !important;
    stroke: var(--vscode-foreground) !important;
    stroke-width: 6.0px !important; /* keep hover slightly stronger than base */
    filter: drop-shadow(0 0 0.25rem rgba(0, 0, 0, 0.52));
  }
`;

const MermaidHeader: React.FC = () => {
  return (
    <div className="pt-4">
    </div>
  );
};

export const MermaidGraphView: React.FC = () => {
  const { graph } = useAtomValue(functionGraphAtom);
  const mermaidRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const panZoomRef = useRef<ReturnType<typeof svgPanZoom> | null>(null);

  const zoomIn = useCallback(() => {
    panZoomRef.current?.zoomBy(1.2);
  }, []);
  const zoomOut = useCallback(() => {
    panZoomRef.current?.zoomBy(1 / 1.2);
  }, []);
  const resetView = useCallback(() => {
    if (!panZoomRef.current) return;
    panZoomRef.current.resetZoom();
    panZoomRef.current.resize();
    panZoomRef.current.fit();
    panZoomRef.current.center();
  }, []);

  useEffect(() => {
    if (!graph || !mermaidRef.current) return;

    let isCancelled = false;
    let resizeObserver: ResizeObserver | null = null;

    // Raw graph string for debugging
    try {
      console.log('[MermaidGraphView] raw graph string:', graph);
    } catch {}

    const onResize = () => {
      if (!panZoomRef.current) return;
      panZoomRef.current.resize();
      panZoomRef.current.fit();
      panZoomRef.current.center();
    };
    window.addEventListener('resize', onResize);

    (async () => {
      try {
        mermaid.initialize({
          startOnLoad: false,
          elk: {
            mergeEdges: false,
            nodePlacementStrategy: 'SIMPLE',
            cycleBreakingStrategy: 'GREEDY',
          },
          theme: 'dark',
          themeCSS: '.mermaid svg { max-width: none !important; }',
          flowchart: {
            arrowMarkerAbsolute: true,
            diagramPadding: 0,
            htmlLabels: true,
            // nodeSpacing: 42,
            rankSpacing: 10,
            curve: 'monotoneX',
            padding: 14,
            defaultRenderer: 'elk',
            wrappingWidth: 220,
            inheritDir: true,
          },
          securityLevel: 'loose',
        });

        // Render via mermaid.render to get SVG string with id
        // Helper to trigger span navigation/flash by nodeId
        const triggerSpan = (nodeId?: string) => {
          try {
            const map = (window as any).__bamlSpanMap as Record<string, any> | undefined;
            if (!map) {
              console.warn('[MermaidGraphView] triggerSpan: no span map present');
              return;
            }
            const span = nodeId ? map[nodeId] : undefined;
            if (!span) {
              console.warn('[MermaidGraphView] triggerSpan: unknown nodeId', nodeId);
              return;
            }
            console.log('[MermaidGraphView] triggerSpan', { nodeId, span });
            vscode.postMessage?.({
              command: 'jumpToFile',
              span: {
                start: span.start,
                end: span.end,
                source_file: span.file_path,
                value: `${(span.file_path.split('/').pop() || '<file>.baml')}:${span.start_line + 1}`,
              },
            });
            window.postMessage(
              {
                command: 'set_flashing_regions',
                content: {
                  spans: [
                    {
                      file_path: span.file_path,
                      start_line: span.start_line,
                      start: span.start,
                      end_line: span.end_line,
                      end: span.end,
                    },
                  ],
                },
              },
              '*',
            );
          } catch (err) {
            console.error('[MermaidGraphView] error in triggerSpan', err);
          }
        };

        // Define the global callback expected by the generated Mermaid graph
        (window as any).bamlMermaidNodeClick = (nodeId?: string) => {
          console.log('[MermaidGraphView] global callback fired', nodeId);
          triggerSpan(nodeId);
        };

        const { svg } = await mermaid.render('bamlMermaidSvg', graph);
        if (isCancelled || !mermaidRef.current) return;

        const normalizedSvg = svg
          .replace(/[ ]*max-width:[ 0-9\.]*px;/i, '')
          .replace(/width="[0-9\.]+px"/i, '')
          .replace(/height="[0-9\.]+px"/i, '');
        mermaidRef.current.innerHTML = normalizedSvg;

        const svgEl = mermaidRef.current.querySelector('#bamlMermaidSvg') as SVGSVGElement | null;
        if (!svgEl) return;
        svgRef.current = svgEl;
        // Make the SVG fill the container
        svgEl.setAttribute('width', '100%');
        svgEl.setAttribute('height', '100%');
        svgEl.setAttribute('preserveAspectRatio', 'xMidYMid meet');
        svgEl.style.width = '100%';
        svgEl.style.height = '100%';
        svgEl.style.display = 'block';

        // Inject CSS override so it takes precedence over Mermaid defaults inside the SVG
        try {
          const styleEl = document.createElement('style');
          styleEl.setAttribute('data-baml', 'mermaid-css-override');
          styleEl.textContent = MERMAID_CSS_OVERRIDE;
          svgEl.appendChild(styleEl);
        } catch {}

        // Programmatically round rect corners (CSS cannot set rx/ry reliably on SVG rects)
        try {
          svgEl.querySelectorAll('g.node rect, g.cluster rect, .edgeLabel rect').forEach((el) => {
            (el as SVGRectElement).setAttribute('rx', '10');
            (el as SVGRectElement).setAttribute('ry', '10');
          });
        } catch {}

        // Expand cluster label foreignObjects to fit content after font/styling changes
        try {
          svgEl.querySelectorAll('g .cluster-label').forEach((label) => {
            const fo = label.querySelector('foreignObject') as SVGForeignObjectElement | null;
            const div = fo?.querySelector('div') as HTMLElement | null;
            if (fo && div) {
              div.style.display = 'inline-block';
              div.style.whiteSpace = 'nowrap';
              div.style.maxWidth = 'none';
              // Measure actual content size in CSS pixels
              const rect = div.getBoundingClientRect();
              // Use measured size to update the foreignObject box (SVG user units ~= CSS px here)
              if (rect.width > 0) fo.setAttribute('width', String(rect.width));
              if (rect.height > 0) fo.setAttribute('height', String(rect.height));
            }
          });
        } catch {}

        // Keep arrowhead markers near default sizing (no scaling), only color is overridden via CSS

        // Initialize svg-pan-zoom on the generated SVG
        panZoomRef.current = svgPanZoom(svgEl as unknown as SVGElement, {
          panEnabled: true,
          zoomEnabled: true,
          controlIconsEnabled: false,
          fit: true,
          center: true,
          minZoom: 0.25,
          maxZoom: 4,
          zoomScaleSensitivity: 0.3,
          preventMouseEventsDefault: false,
        });

        // After init, ensure proper fit/center
        panZoomRef.current.resize();
        panZoomRef.current.fit();
        panZoomRef.current.center();

        // Prevent wheel from scrolling parent while zooming/panning
        const wheelPreventer = (e: WheelEvent) => {
          e.preventDefault();
        };
        // Attach to the container hosting the SVG
        svgEl.addEventListener('wheel', wheelPreventer, { passive: false });

        // Parse span map from a special mermaid comment emitted at the end
        // Look for a comment node like %%__BAML_SPANMAP__={...}
        const raw = graph as string;
        const spanMapMatch = raw.match(/%%__BAML_SPANMAP__=(\{[\s\S]*\})/);
        let spanMap: Record<string, {
          file_path: string;
          start_line: number;
          start: number;
          end_line: number;
          end: number;
        }> | null = null;
        if (spanMapMatch && spanMapMatch[1]) {
          try {
            spanMap = JSON.parse(spanMapMatch[1]);
          } catch (_e) {
            spanMap = null;
          }
        }

        // Expose the span map globally for the callback to use
        if (spanMap) {
          (window as any).__bamlSpanMap = spanMap;
          console.log('[MermaidGraphView] span map ready', Object.keys(spanMap));
          // Also attach direct click handlers to avoid interference from pan/zoom
          const manualListeners: Array<{ el: Element; fn: (ev: Event) => void }> = [];
          Object.entries(spanMap).forEach(([nodeId, span]) => {
            const candidates = [
              `#${nodeId}`,
              `#flowchart-${nodeId}`,
              `g[id^="${nodeId}"]`,
              `g[id*="-${nodeId}"]`,
              `g[id^="flowchart-${nodeId}"]`,
            ];
            const target = candidates
              .map((sel) => svgEl.querySelector(sel) as SVGElement | null)
              .find((el) => !!el);
            if (!target) return;
            target.style.cursor = 'pointer';
            try { target.setAttribute('data-baml-node-id', nodeId); } catch {}
            const onClick = (ev: Event) => {
              ev.stopPropagation();
              console.log('[MermaidGraphView] node click (manual handler)', { nodeId, span, target: (ev.target as Element)?.tagName });
              triggerSpan(nodeId);
            };
            target.addEventListener('click', onClick);
            manualListeners.push({ el: target, fn: onClick });
          });
          // Ensure all visible mermaid nodes show pointer cursor for UX/debug
          svgEl.querySelectorAll('.node').forEach((el) => {
            (el as SVGGraphicsElement).style.cursor = 'pointer';
            (el as SVGGraphicsElement).style.pointerEvents = 'all';
          });
          // Attach generic listener to any node group; derive nodeId from its id content
          const extractNodeId = (rawId?: string | null): string | null => {
            if (!rawId) return null;
            let id = rawId;
            if (id.startsWith('flowchart-')) {
              id = id.slice('flowchart-'.length);
            }
            const firstSegment = id.split('-')[0];
            return firstSegment || null;
          };

          const generic = (ev: Event) => {
            const el = ev.target as Element | null;
            if (!el) return;
            const g = el.closest('g[id]') as Element | null;
            if (!g) return;
            const rawId = g.getAttribute('id');
            const baseId = extractNodeId(rawId);
            let key: string | undefined;
            if (baseId && spanMap![baseId]) {
              key = baseId;
            } else {
              const dataId = g.getAttribute('data-baml-node-id');
              if (dataId && spanMap![dataId]) key = dataId;
            }
            if (key) {
              console.log('[MermaidGraphView] node click (generic)', { rawId, baseId, key });
              triggerSpan(key);
            }
          };
          svgEl.querySelectorAll('g.node').forEach((g) => {
            g.addEventListener('click', generic);
            manualListeners.push({ el: g, fn: generic });
          });

          // Cleanup manual listeners on unmount
          (window as any).__bamlCleanupListeners = () => {
            manualListeners.forEach(({ el, fn }) => {
              el.removeEventListener('click', fn);
            });
          };
        }

        // Observe container size changes (ResizablePanel drags)
        if (containerRef.current && typeof ResizeObserver !== 'undefined') {
          resizeObserver = new ResizeObserver(() => {
            if (!panZoomRef.current) return;
            panZoomRef.current.resize();
            panZoomRef.current.fit();
            panZoomRef.current.center();
          });
          resizeObserver.observe(containerRef.current);
        }
      } catch (_err) {
        // swallow render errors; UI may show diagnostics elsewhere
      }
    })();

    return () => {
      isCancelled = true;
      window.removeEventListener('resize', onResize);
      if (resizeObserver) resizeObserver.disconnect();
      panZoomRef.current?.destroy();
      panZoomRef.current = null;
      try {
        const svgEl = svgRef.current;
        if (svgEl) {
          svgEl.removeEventListener('wheel', (e: any) => e.preventDefault());
        }
        if ((window as any).__bamlCleanupListeners) {
          (window as any).__bamlCleanupListeners();
          delete (window as any).__bamlCleanupListeners;
        }
      } catch {}
    };
  }, [graph]);

  return (
    <div className="flex flex-col w-full h-full min-h-0">
      <MermaidHeader />
      <div
        ref={containerRef}
        className="relative flex-1 min-h-0 overflow-hidden border rounded bg-transparent"
        style={{
          borderColor: 'var(--vscode-panel-border)',
          backgroundColor: 'transparent',
        }}
      >
        <div
          ref={mermaidRef}
          className="absolute inset-0 overflow-hidden p-2"
        />

        {/* Controls overlay */}
        <div className="absolute top-2 right-2 z-10">
          <div className="flex flex-col items-center gap-1 p-2 rounded border bg-[var(--vscode-editor-background)] border-[var(--vscode-panel-border)] shadow">
            <Button size="icon" variant="outline" onClick={zoomIn} className="h-8 w-8 p-0" title="Zoom In">
              <ZoomIn className="h-4 w-4" />
            </Button>
            <Button size="icon" variant="outline" onClick={zoomOut} className="h-8 w-8 p-0" title="Zoom Out">
              <ZoomOut className="h-4 w-4" />
            </Button>
            <Button size="icon" variant="outline" onClick={() => resetView()} className="h-8 w-8 p-0" title="Reset View">
              <Maximize2 className="h-4 w-4" />
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
};


