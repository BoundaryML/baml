export const SIDEBAR_LEAF_ROW_CLASS =
  'flex items-center gap-1 w-full pr-2 py-1 text-[11px] font-vsc-mono text-left';

export const SIDEBAR_LEAF_ICON_CLASS =
  'h-3.5 w-3.5 shrink-0 text-vsc-text-faint';

export function getSidebarLeafPaddingLeft(depth = 0): number {
  return 20 + depth * 12;
}
