// Where files sit on the stage.
//
// Each directory with touched files is a frame; its files are cards inside
// it. Frames are ordered by path, so sibling directories sit next to each
// other and the same project always lays out the same way: a mental map you
// can learn. Frames are packed into rows that wrap at a fixed world width.

import type { Rect } from './camera';

export const CARD_W = 594;
export const CARD_H = 452;
const CARD_GAP = 20;
const FRAME_PAD = 20;
const FRAME_GAP_X = 64;
const ROW_GAP = 96;
const ROW_WIDTH = 2400;

export interface FrameBox extends Rect {
  dir: string;
  files: string[];
}

export interface StageLayout {
  frames: FrameBox[];
  cards: Map<string, Rect>;
  bounds: Rect;
}

export function layoutStage(paths: string[]): StageLayout {
  const byDir = new Map<string, string[]>();
  for (const path of paths) {
    const slash = path.lastIndexOf('/');
    const dir = slash < 0 ? '' : path.slice(0, slash);
    const list = byDir.get(dir) ?? [];
    list.push(path);
    byDir.set(dir, list);
  }
  const dirs = [...byDir.keys()].sort((a, b) => a.localeCompare(b));

  const frames: FrameBox[] = [];
  const cards = new Map<string, Rect>();
  let x = 0;
  let y = 0;
  let rowHeight = 0;
  for (const dir of dirs) {
    const files = byDir.get(dir)!.sort((a, b) => a.localeCompare(b));
    const cols = files.length <= 2 ? files.length : files.length <= 6 ? 2 : 3;
    const rows = Math.ceil(files.length / cols);
    const w = cols * CARD_W + (cols - 1) * CARD_GAP + FRAME_PAD * 2;
    const h = rows * CARD_H + (rows - 1) * CARD_GAP + FRAME_PAD * 2;
    if (x > 0 && x + w > ROW_WIDTH) {
      x = 0;
      y += rowHeight + ROW_GAP;
      rowHeight = 0;
    }
    frames.push({ dir, files, h, w, x, y });
    files.forEach((path, i) => {
      const col = i % cols;
      const row = Math.floor(i / cols);
      cards.set(path, {
        h: CARD_H,
        w: CARD_W,
        x: x + FRAME_PAD + col * (CARD_W + CARD_GAP),
        y: y + FRAME_PAD + row * (CARD_H + CARD_GAP),
      });
    });
    x += w + FRAME_GAP_X;
    rowHeight = Math.max(rowHeight, h);
  }
  const bounds: Rect = frames.length
    ? {
        h: Math.max(...frames.map((f) => f.y + f.h)) + 40,
        w: Math.max(...frames.map((f) => f.x + f.w)),
        x: 0,
        y: -40,
      }
    : { h: CARD_H, w: CARD_W, x: 0, y: 0 };
  return { bounds, cards, frames };
}
