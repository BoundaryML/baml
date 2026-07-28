import { describe, expect, it } from 'vitest';
import {
  getSidebarLeafPaddingLeft,
  SIDEBAR_LEAF_ICON_CLASS,
  SIDEBAR_LEAF_ROW_CLASS,
} from './function-test-sidebar-row-styles';

describe('function/test sidebar leaf row styles', () => {
  it('defines one row geometry for function and test leaves', () => {
    expect(SIDEBAR_LEAF_ROW_CLASS).toContain('gap-1');
    expect(SIDEBAR_LEAF_ROW_CLASS).toContain('py-1');
    expect(SIDEBAR_LEAF_ROW_CLASS).toContain('text-[11px]');
    expect(SIDEBAR_LEAF_ROW_CLASS).toContain('font-vsc-mono');
  });

  it('defines one icon geometry for function and test leaves', () => {
    expect(SIDEBAR_LEAF_ICON_CLASS).toContain('h-3.5');
    expect(SIDEBAR_LEAF_ICON_CLASS).toContain('w-3.5');
    expect(SIDEBAR_LEAF_ICON_CLASS).toContain('shrink-0');
  });

  it('aligns top-level leaves and advances nested leaves by one tree step', () => {
    expect(getSidebarLeafPaddingLeft()).toBe(20);
    expect(getSidebarLeafPaddingLeft(1)).toBe(32);
    expect(getSidebarLeafPaddingLeft(2)).toBe(44);
  });
});
