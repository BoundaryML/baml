import { describe, expect, it, vi } from 'vitest';

import {
  rowActivation,
  stopRowActivation,
  tableRowActivation,
} from './TelemetryView';

function keyEvent(key: string) {
  return {
    key,
    preventDefault: vi.fn(),
  } as unknown as Parameters<
    ReturnType<typeof rowActivation>['onKeyDown']
  >[0] & {
    preventDefault: ReturnType<typeof vi.fn>;
  };
}

describe('rowActivation', () => {
  it('activates on click', () => {
    const onActivate = vi.fn();
    rowActivation(onActivate).onClick();
    expect(onActivate).toHaveBeenCalledTimes(1);
  });

  it('activates on Enter and Space', () => {
    for (const key of ['Enter', ' ']) {
      const onActivate = vi.fn();
      const event = keyEvent(key);
      rowActivation(onActivate).onKeyDown(event);
      expect(onActivate).toHaveBeenCalledTimes(1);
      // Space would otherwise scroll the pane and lose the reader's place.
      expect(event.preventDefault).toHaveBeenCalled();
    }
  });

  it('ignores other keys, so typing in the row does not select it', () => {
    const onActivate = vi.fn();
    const event = keyEvent('a');
    rowActivation(onActivate).onKeyDown(event);
    expect(onActivate).not.toHaveBeenCalled();
    expect(event.preventDefault).not.toHaveBeenCalled();
  });

  it('exposes the row as a focusable control', () => {
    const props = rowActivation(vi.fn());
    expect(props.role).toBe('button');
    expect(props.tabIndex).toBe(0);
  });
});

describe('tableRowActivation', () => {
  it('clicks without claiming to be a button', () => {
    const onActivate = vi.fn();
    const props = tableRowActivation(onActivate);
    props.onClick();
    expect(onActivate).toHaveBeenCalledTimes(1);
    // A `tr` announced as a button loses the column context that makes a
    // table readable non-visually; the in-row control stays the a11y path.
    expect('role' in props).toBe(false);
    expect('tabIndex' in props).toBe(false);
  });
});

describe('stopRowActivation', () => {
  it('keeps a nested control from also selecting the row', () => {
    const event = { stopPropagation: vi.fn() };
    stopRowActivation(event);
    expect(event.stopPropagation).toHaveBeenCalledTimes(1);
  });
});
