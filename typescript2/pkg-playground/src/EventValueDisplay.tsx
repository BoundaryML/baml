/**
 * Thin wrapper that renders a BamlJsValue through ValueRenderer with inline display mode.
 * Used to render event payloads (log.data, custom.data) inside event rows.
 */

import type { FC } from 'react';
import type { BamlJsValue } from '@b/pkg-proto';
import type { ResultRendererProps } from './result-renderers';
import { ValueRenderer } from './ValueRenderer';

export const EventValueDisplay: FC<{
  value: BamlJsValue;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
}> = ({ value, customRenderers }) => (
  <ValueRenderer value={value} displayMode="inline" customRenderers={customRenderers} />
);
