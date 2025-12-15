import { atom } from 'jotai';

/**
 * Atom to store the current search query for prompt preview
 */
export const promptSearchQueryAtom = atom<string>('');

/**
 * Atom to control whether the search bar is visible
 */
export const promptSearchVisibleAtom = atom<boolean>(false);

/**
 * Atom to store the current match index (for navigation between matches)
 */
export const promptSearchCurrentMatchAtom = atom<number>(0);

/**
 * Match registration entry with count and order for computing global offsets
 */
interface MatchRegistration {
  count: number;
  order: number; // Used to sort components and compute offsets
}

/**
 * Atom to store match counts from individual components
 * Key is a unique component ID, value includes count and order
 */
export const promptSearchMatchCountsAtom = atom<Map<string, MatchRegistration>>(new Map());

/**
 * Derived atom to compute total matches from all components
 */
export const promptSearchTotalMatchesAtom = atom(
  (get) => {
    const matchCounts = get(promptSearchMatchCountsAtom);
    let total = 0;
    matchCounts.forEach((reg) => {
      total += reg.count;
    });
    return total;
  }
);

/**
 * Derived atom to get sorted entries for computing offsets
 */
export const sortedMatchEntriesAtom = atom(
  (get) => {
    const matchCounts = get(promptSearchMatchCountsAtom);
    const entries = Array.from(matchCounts.entries());
    // Sort by order to ensure consistent global indices
    entries.sort((a, b) => a[1].order - b[1].order);
    return entries;
  }
);

/**
 * Derived atom to compute the global offset for each component
 * Returns a map of component ID -> global offset (start index for that component's matches)
 */
export const matchOffsetsAtom = atom(
  (get) => {
    const sortedEntries = get(sortedMatchEntriesAtom);
    const offsets = new Map<string, number>();
    let cumulativeOffset = 0;

    for (const [id, reg] of sortedEntries) {
      offsets.set(id, cumulativeOffset);
      cumulativeOffset += reg.count;
    }

    return offsets;
  }
);

// Global counter for registration order
let registrationCounter = 0;

/**
 * Write-only atom to register a component's match count
 */
export const registerMatchCountAtom = atom(
  null,
  (get, set, { id, count, order }: { id: string; count: number; order?: number }) => {
    const current = get(promptSearchMatchCountsAtom);
    const next = new Map(current);

    // If component already registered, preserve its order
    const existing = current.get(id);
    const finalOrder = order ?? existing?.order ?? registrationCounter++;

    if (count > 0) {
      next.set(id, { count, order: finalOrder });
    } else {
      next.delete(id);
    }
    set(promptSearchMatchCountsAtom, next);
  }
);

/**
 * Write-only atom to unregister a component's match count
 */
export const unregisterMatchCountAtom = atom(
  null,
  (get, set, id: string) => {
    const current = get(promptSearchMatchCountsAtom);
    const next = new Map(current);
    next.delete(id);
    set(promptSearchMatchCountsAtom, next);
  }
);

/**
 * Write-only atom to clear all match counts (useful when switching tabs)
 */
export const clearMatchCountsAtom = atom(
  null,
  (_get, set) => {
    set(promptSearchMatchCountsAtom, new Map());
    registrationCounter = 0; // Reset counter when clearing
  }
);
