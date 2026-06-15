/**
 * Renders a Playground result using registered custom renderers when values
 * have a $baml.type, otherwise falls back to formatted JSON.
 *
 * Recursively walks objects/arrays so nested $baml-typed values (e.g. media
 * inside a class) are rendered with their registered component.
 */

import type { FC } from 'react';
import type { BamlJsValue } from '@b/pkg-proto';
import type { ResultRendererProps } from './result-renderers';
import { ValueRenderer } from './ValueRenderer';

export interface ResultDisplayProps {
  /** Deserialized result value. */
  result: BamlJsValue;
  /** Optional extra renderers (type -> Component) merged with registry. */
  customRenderers?: Record<string, FC<ResultRendererProps>>;
}

export const ResultDisplay: FC<ResultDisplayProps> = ({
  result,
  customRenderers,
}) => {
  return (
    <ValueRenderer
      value={result}
      customRenderers={customRenderers}
      displayMode="expanded"
    />
  );
};
