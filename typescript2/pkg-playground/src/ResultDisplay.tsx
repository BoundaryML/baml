/**
 * Renders a Playground result using registered custom renderers when values
 * have a $baml.type, otherwise falls back to formatted JSON.
 *
 * Recursively walks objects/arrays so nested $baml-typed values (e.g. media
 * inside a class) are rendered with their registered component.
 */

import type { FC } from 'react';
import { getBamlType, getResultRenderer, BAML_TYPE_KEY } from './result-renderers';
import type { ResultRendererProps } from './result-renderers';

const codeBlockCls =
  'whitespace-pre-wrap break-all font-vsc-mono text-xs leading-relaxed p-2 rounded bg-vsc-bg border border-vsc-border text-vsc-text overflow-auto max-h-[200px] m-0';

export interface ResultDisplayProps {
  /** Raw result JSON string from the runtime. */
  resultJson: string;
  /** Optional extra renderers (type -> Component) merged with registry. */
  customRenderers?: Record<string, FC<ResultRendererProps>>;
}

function resolve(
  type: string,
  customRenderers?: Record<string, FC<ResultRendererProps>>,
): FC<ResultRendererProps> | undefined {
  return customRenderers?.[type] ?? getResultRenderer(type);
}

const ValueRenderer: FC<{ value: unknown; customRenderers?: Record<string, FC<ResultRendererProps>> }> = ({
  value,
  customRenderers,
}) => {
  if (value == null || typeof value !== 'object') {
    return <span className="font-vsc-mono text-xs text-vsc-text">{JSON.stringify(value)}</span>;
  }

  const type = getBamlType(value);
  if (type) {
    const Renderer = resolve(type, customRenderers);
    if (Renderer) return <Renderer value={value} />;
    return <pre className={codeBlockCls}>{JSON.stringify(value, null, 2)}</pre>;
  }

  if (Array.isArray(value)) {
    return (
      <div className="space-y-1 pl-2 border-l border-vsc-border-subtle">
        {value.map((item, i) => (
          <ValueRenderer key={i} value={item} customRenderers={customRenderers} />
        ))}
      </div>
    );
  }

  // Plain object — render each field, recursing into values
  const entries = Object.entries(value as Record<string, unknown>).filter(
    ([k]) => k !== BAML_TYPE_KEY,
  );
  if (entries.length === 0) {
    return <span className="font-vsc-mono text-xs text-vsc-text-faint">{'{}'}</span>;
  }

  return (
    <div className="space-y-1">
      {entries.map(([key, val]) => {
        const isComplex = val != null && typeof val === 'object';

        if (!isComplex) {
          return (
            <div key={key} className="flex gap-1.5 items-baseline font-vsc-mono text-xs">
              <span className="text-vsc-text-muted shrink-0">{key}:</span>
              <span className="text-vsc-text">{JSON.stringify(val)}</span>
            </div>
          );
        }

        return (
          <div key={key} className="space-y-0.5">
            <div className="font-vsc-mono text-xs text-vsc-text-muted">{key}:</div>
            <div className="pl-2">
              <ValueRenderer value={val} customRenderers={customRenderers} />
            </div>
          </div>
        );
      })}
    </div>
  );
};

export const ResultDisplay: FC<ResultDisplayProps> = ({ resultJson, customRenderers }) => {
  let value: unknown;
  try {
    value = JSON.parse(resultJson);
  } catch {
    return <pre className={codeBlockCls}>{resultJson}</pre>;
  }

  return <ValueRenderer value={value} customRenderers={customRenderers} />;
};
