export interface TreeData {
  parentById: Record<string, string | undefined>;
  childrenById: Record<string, string[]>;
}

export const findCommonAncestor = (
  id1: string,
  id2: string,
  { parentById }: TreeData,
) => {
  const visited: Set<string> = new Set();
  let currentId: string | undefined = id1;

  // Edge case with self edges
  if (id1 === id2) {
    return parentById[id1] ?? 'root';
  }

  while (currentId) {
    visited.add(currentId);
    if (currentId === id2) {
      return currentId;
    }
    currentId = parentById[currentId];
  }

  currentId = id2;
  while (currentId) {
    if (visited.has(currentId)) {
      return currentId;
    }
    currentId = parentById[currentId];
  }

  return 'root';
};
