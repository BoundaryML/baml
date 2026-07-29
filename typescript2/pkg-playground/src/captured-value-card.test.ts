import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import {
  CapturedValueCard,
  capturedValueCardContentHeight,
  capturedValueCardDiagnosticHeight,
} from './CapturedValueCard';
import { graphValuePreviewHeight } from './graph/layout';
import {
  NODE_VALUE_PREVIEW_FOOTER_HEIGHT,
  NODE_VALUE_PREVIEW_GAP,
} from './graph/nodes/NodeOutputPreview';
import type { WorkflowNode } from './graph/types';
import type { RunTraceCallValue } from './run-store-projections';

describe('captured value card graph sizing', () => {
  it('allocates one row per traceback line plus wrapped rows', () => {
    expect(capturedValueCardDiagnosticHeight('header\nshort')).toBe(33);
    expect(capturedValueCardDiagnosticHeight(`header\n${'x'.repeat(55)}`)).toBe(
      47,
    );
  });

  it('allocates expanded rows for nested input values', () => {
    expect(
      capturedValueCardContentHeight(
        preview({
          repo_root: {
            $baml: { type: 'baml.trace.OmittedValue' },
            message: 'omitted argument',
            reason: 'omittedArgument',
          },
        }),
      ),
    ).toBe(120);
  });

  it('uses the smaller expanded height for a one-field typed error', () => {
    expect(
      capturedValueCardContentHeight(
        preview({
          $baml: { type: 'baml.errors.InvalidArgument' },
          message: 'invalid argument',
        }),
      ),
    ).toBe(48);
  });

  it('left-aligns pretty-printed graph values inside centered React Flow groups', () => {
    const markup = renderToStaticMarkup(
      createElement(CapturedValueCard, {
        prettyPrintValue: true,
        value: preview({ message: 'invalid argument' }),
      }),
    );

    expect(markup).toContain('text-align:left');
  });

  it('reserves a gap and footer row when five previews are truncated to four', () => {
    const fourPreviewsHeight = graphValuePreviewHeight(nodeWithPreviews(4));
    const fivePreviewsHeight = graphValuePreviewHeight(nodeWithPreviews(5));

    expect(fivePreviewsHeight - fourPreviewsHeight).toBe(
      NODE_VALUE_PREVIEW_GAP + NODE_VALUE_PREVIEW_FOOTER_HEIGHT,
    );
  });
});

function preview(value: unknown): RunTraceCallValue {
  return {
    diagnostic: null,
    id: 'preview',
    label: 'input',
    role: 'callInput',
    state: 'available',
    timestampMs: 0,
    value: value as RunTraceCallValue['value'],
    valueRef: null,
  };
}

function nodeWithPreviews(count: number): WorkflowNode {
  return {
    data: {
      executionState: 'success',
      graphNodeType: 'function',
      label: 'preview',
      logFilterKey: 'preview',
      selected: false,
      valuePreviews: Array.from({ length: count }, (_, index) =>
        preview(`value-${index}`),
      ),
    },
    id: 'preview',
    position: { x: 0, y: 0 },
    type: 'base',
  };
}
