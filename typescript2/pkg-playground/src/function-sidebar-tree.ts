import type { FunctionInfo } from './worker-protocol';

export type FunctionSidebarFunctionNode = {
  type: 'function';
  functionInfo: FunctionInfo;
  fullName: string;
  label: string;
  key: string;
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

type MutableFolderNode = Omit<FunctionSidebarFolderNode, 'children'> & {
  children: Array<FunctionSidebarTreeNode | MutableFolderNode>;
  foldersByName: Map<string, MutableFolderNode>;
};

type BuildFunctionSidebarTreeOptions = {
  search?: string;
  selectedFunctionName?: string | null;
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

  for (const functionInfo of functions) {
    if (query && !functionInfo.name.toLowerCase().includes(query)) continue;

    const parts = functionInfo.name.split('.');
    const label = parts[parts.length - 1] ?? functionInfo.name;
    const namespacePath = parts.slice(0, -1);
    const leaf: FunctionSidebarFunctionNode = {
      type: 'function',
      functionInfo,
      fullName: functionInfo.name,
      label,
      key: functionInfo.name,
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
    nodes: root.map(finalizeNode),
    functionCount,
    forcedOpenFolderKeys,
  };
}

function createFolder(name: string, path: string[]): MutableFolderNode {
  return {
    type: 'folder',
    name,
    path,
    key: path.join('.'),
    functionCount: 0,
    children: [],
    foldersByName: new Map(),
  };
}

function finalizeNode(
  node: FunctionSidebarTreeNode | MutableFolderNode,
): FunctionSidebarTreeNode {
  if (node.type === 'function') return node;
  return {
    type: 'folder',
    name: node.name,
    path: node.path,
    key: node.key,
    functionCount: node.functionCount,
    children: node.children.map(finalizeNode),
  };
}

function addForcedOpenFolders(keys: Set<string>, namespacePath: string[]) {
  for (let index = 1; index <= namespacePath.length; index += 1) {
    keys.add(namespacePath.slice(0, index).join('.'));
  }
}
