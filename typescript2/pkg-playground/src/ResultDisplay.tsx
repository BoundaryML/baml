/**
 * Renders a Playground result using registered custom renderers when values
 * have a $baml.type, otherwise falls back to formatted JSON.
 *
 * Recursively walks objects/arrays so nested $baml-typed values (e.g. media
 * inside a class) are rendered with their registered component.
 */

import type { FC } from 'react';
import { CodeBlock } from './components/ui/code-block';
import type { ResultRendererProps } from './result-renderers';
import { ValueRenderer } from './ValueRenderer';

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
    return <CodeBlock>{resultJson}</CodeBlock>;
  }

  return <ValueRenderer value={value} customRenderers={customRenderers} />;
};
