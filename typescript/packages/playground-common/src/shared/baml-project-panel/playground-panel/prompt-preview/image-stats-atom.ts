import { atom } from 'jotai';

export interface ImageStatsData {
  width: number;
  height: number;
  size: string;
  url: string;
}

// Map of image URL to its stats
export const imageStatsMapAtom = atom<Map<string, ImageStatsData>>(new Map());