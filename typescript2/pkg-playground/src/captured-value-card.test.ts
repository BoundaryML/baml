import { describe, expect, it } from 'vitest';

import {
  capturedValueCardContentHeight,
  capturedValueCardDiagnosticHeight,
} from './CapturedValueCard';
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
