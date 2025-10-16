import { atom } from 'jotai';

// Map of media content (as unique key) to its collapsed state
export const mediaCollapsedMapAtom = atom<Map<string, boolean>>(new Map()); 