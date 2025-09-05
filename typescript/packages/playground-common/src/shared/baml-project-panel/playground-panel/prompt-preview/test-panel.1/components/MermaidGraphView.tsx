import { useAtomValue } from 'jotai';
import { useEffect, useRef } from 'react';
import mermaid from 'mermaid';
import { functionGraphAtom } from '../../../atoms-orch-graph';

const MermaidHeader: React.FC = () => {
  return (
    <div className="pt-4">
      <div className="text-sm font-bold">Function Flow Diagram</div>
      <div className="flex flex-col-reverse items-start gap-0.5">
        <span className="pl-2 text-xs text-muted-foreground flex flex-row flex-wrap items-center gap-0.5">
          Mermaid diagram visualization
        </span>
      </div>
    </div>
  );
};

export const MermaidGraphView: React.FC = () => {
  const { graph } = useAtomValue(functionGraphAtom);
  const mermaidRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (graph && mermaidRef.current) {
      mermaid.initialize({
        startOnLoad: true,
        theme: 'dark',
        themeVariables: {
          primaryColor: '#1e293b',
          primaryTextColor: '#e2e8f0',
          primaryBorderColor: '#475569',
          lineColor: '#64748b',
          secondaryColor: '#334155',
          tertiaryColor: '#1e293b',
          background: '#0f172a',
          mainBkg: '#1e293b',
          secondBkg: '#334155',
          tertiaryBkg: '#0f172a',
          secondaryBorderColor: '#64748b',
          tertiaryBorderColor: '#334155',
          textColor: '#e2e8f0',
          nodeTextColor: '#e2e8f0',
          labelTextColor: '#cbd5e1',
          errorBkgColor: '#7f1d1d',
          errorTextColor: '#fca5a5',
        },
        flowchart: {
          nodeSpacing: 50,
          rankSpacing: 50,
          curve: 'basis',
        },
      });

      mermaidRef.current.innerHTML = '';
      const graphElement = document.createElement('div');
      graphElement.className = 'mermaid';
      graphElement.textContent = graph;
      mermaidRef.current.appendChild(graphElement);

      mermaid.run({ nodes: [graphElement] });
    }
  }, [graph]);

  return (
    <div className="flex flex-col w-full h-full">
      <MermaidHeader />
      <div 
        ref={mermaidRef} 
        className="h-full overflow-auto p-4 flex items-center justify-center"
        style={{
          backgroundColor: '#0f172a',
        }}
      />
    </div>
  );
};