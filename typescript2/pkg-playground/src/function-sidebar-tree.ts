import type { FunctionInfo } from './worker-protocol';

export type FunctionSidebarFunctionNode = {
  type: 'function';
  functionInfo: FunctionInfo;
  fullName: string;
  label: string;
  key: string;
  workflowNodeCount?: number;
};

export type FunctionSidebarFolderNode = {
  type: 'folder';
  name: string;
  path: string[];
  key: string;
  functionCount: number;
  children: FunctionSidebarTreeNode[];
};

export type FunctionSidebarTreeNode =
  | FunctionSidebarFunctionNode
  | FunctionSidebarFolderNode;

export type FunctionSidebarTree = {
  nodes: FunctionSidebarTreeNode[];
  functionCount: number;
  forcedOpenFolderKeys: Set<string>;
};

export type FunctionSortOrder = 'alphanumeric' | 'workflowNodeCount';

const naturalFunctionNameCollator = new Intl.Collator('en', {
  numeric: true,
  sensitivity: 'base',
});

type MutableFolderNode = Omit<FunctionSidebarFolderNode, 'children'> & {
  children: Array<FunctionSidebarTreeNode | MutableFolderNode>;
  foldersByName: Map<string, MutableFolderNode>;
};

type BuildFunctionSidebarTreeOptions = {
  search?: string;
  selectedFunctionName?: string | null;
  sortOrder?: FunctionSortOrder;
  workflowNodeCounts?: ReadonlyMap<string, number>;
};

export function buildFunctionSidebarTree(
  functions: FunctionInfo[],
  options: BuildFunctionSidebarTreeOptions = {},
): FunctionSidebarTree {
  const query = options.search?.trim().toLowerCase() ?? '';
  const root: Array<FunctionSidebarTreeNode | MutableFolderNode> = [];
  const rootFolders = new Map<string, MutableFolderNode>();
  const forcedOpenFolderKeys = new Set<string>();
  let functionCount = 0;

  const workflowNodeCounts = options.workflowNodeCounts;
  const sortOrder = options.sortOrder ?? 'workflowNodeCount';
  const sortedFunctions = functions
    .map((functionInfo, index) => ({ functionInfo, index }))
    .sort((a, b) => {
      if (sortOrder === 'alphanumeric') {
        return (
          naturalFunctionNameCollator.compare(
            a.functionInfo.name,
            b.functionInfo.name,
          ) || a.index - b.index
        );
      }
      if (!workflowNodeCounts) return a.index - b.index;
      return (
        (workflowNodeCounts.get(b.functionInfo.name) ?? 0) -
          (workflowNodeCounts.get(a.functionInfo.name) ?? 0) ||
        a.index - b.index
      );
    })
    .map(({ functionInfo }) => functionInfo);

  for (const functionInfo of sortedFunctions) {
    if (query && !functionInfo.name.toLowerCase().includes(query)) continue;

    const parts = functionInfo.name.split('.');
    const label = parts.at(-1) ?? functionInfo.name;
    const namespacePath = parts.slice(0, -1);
    const leaf: FunctionSidebarFunctionNode = {
      fullName: functionInfo.name,
      functionInfo,
      key: functionInfo.name,
      label,
      type: 'function',
      workflowNodeCount: workflowNodeCounts?.get(functionInfo.name),
    };

    functionCount += 1;
    if (query || functionInfo.name === options.selectedFunctionName) {
      addForcedOpenFolders(forcedOpenFolderKeys, namespacePath);
    }

    if (namespacePath.length === 0) {
      root.push(leaf);
      continue;
    }

    let children = root;
    let folders = rootFolders;
    let folder: MutableFolderNode | undefined;
    for (let index = 0; index < namespacePath.length; index += 1) {
      const segment = namespacePath[index]!;
      folder = folders.get(segment);
      if (!folder) {
        folder = createFolder(segment, namespacePath.slice(0, index + 1));
        folders.set(segment, folder);
        children.push(folder);
      }
      folder.functionCount += 1;
      children = folder.children;
      folders = folder.foldersByName;
    }

    children.push(leaf);
  }

  return {
    forcedOpenFolderKeys,
    functionCount,
    nodes: root.map(finalizeNode),
  };
}

function createFolder(name: string, path: string[]): MutableFolderNode {
  return {
    children: [],
    foldersByName: new Map(),
    functionCount: 0,
    key: path.join('.'),
    name,
    path,
    type: 'folder',
  };
}

function finalizeNode(
  node: FunctionSidebarTreeNode | MutableFolderNode,
): FunctionSidebarTreeNode {
  if (node.type === 'function') return node;
  return {
    children: node.children.map(finalizeNode),
    functionCount: node.functionCount,
    key: node.key,
    name: node.name,
    path: node.path,
    type: 'folder',
  };
}

function addForcedOpenFolders(keys: Set<string>, namespacePath: string[]) {
  for (let index = 1; index <= namespacePath.length; index += 1) {
    keys.add(namespacePath.slice(0, index).join('.'));
  }
}
