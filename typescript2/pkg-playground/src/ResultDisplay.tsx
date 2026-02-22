/**
 * Renders a Playground result using a registered custom renderer when the
 * value has a $baml.type, otherwise falls back to formatted JSON.
 */

import type { FC } from 'react';
import { getBamlType, getResultRenderer } from './result-renderers';
import type { ResultRendererProps } from './result-renderers';

const codeBlockCls =
  'whitespace-pre-wrap break-all font-vsc-mono text-xs leading-relaxed p-2 rounded bg-vsc-bg border border-vsc-border text-vsc-text overflow-auto max-h-[200px] m-0';

export interface ResultDisplayProps {
  /** Raw result JSON string from the runtime. */
  resultJson: string;
  /** Optional extra renderers (type -> Component) merged with registry. */
  customRenderers?: Record<string, FC<ResultRendererProps>>;
}

export const ResultDisplay: FC<ResultDisplayProps> = ({ resultJson, customRenderers }) => {
  let value: unknown;
  try {
    value = JSON.parse(resultJson);
  } catch {
    return <pre className={codeBlockCls}>{resultJson}</pre>;
  }

  const type = getBamlType(value);
  const Renderer = type
    ? (customRenderers?.[type] ?? getResultRenderer(type))
    : null;

  if (Renderer) {
    return <Renderer value={value} />;
  }

  return (
    <pre className={codeBlockCls}>
      {JSON.stringify(value, null, 2)}
    </pre>
  );
};
