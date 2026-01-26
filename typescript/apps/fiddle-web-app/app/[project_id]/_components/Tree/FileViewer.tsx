// 1: Uncontrolled Tree
import type React from 'react';
import { useRef, useState } from 'react';

import { type NodeApi, Tree, type TreeApi } from 'react-arborist';

import { bamlFilesTrackedAtom, filesAtom } from '@baml/playground-common';
import { useAtom, useAtomValue, useSetAtom } from 'jotai';
import { FilePlus, FolderPlus } from 'lucide-react';
import useResizeObserver from 'use-resize-observer';
import {
  activeFileNameAtom,
  currentEditorFilesAtom,
  emptyDirsAtom,
} from '../../_atoms/atoms';
import Node from './Node';

export const data = [
  {
    id: '1',
    name: 'public',
    children: [
      {
        id: 'c1-1',
        name: 'index.html',
      },
    ],
  },
  {
    id: '2',
    name: 'src',
    children: [
      {
        id: 'c2-1',
        name: 'App.js',
      },
      {
        id: 'c2-2',
        name: 'index.js',
      },
      { id: 'c2-3', name: 'styles.css' },
    ],
  },
  { id: '3', name: 'package.json' },
  { id: '4', name: 'README.md' },
];

interface TreeNode {
  id: string;
  name: string;
  children?: TreeNode[];
}
const isFile = (path: string) => path.includes('.');

function createTree(filePaths: string[]): TreeNode[] {
  // Sort paths folders first, then files, alphabetically.
  const sortedFilePaths = filePaths.sort((a, b) => {
    const isAFolder = !isFile(a);
    const isBFolder = !isFile(b);

    if (isAFolder && !isBFolder) {
      return -1;
    } else if (!isAFolder && isBFolder) {
      return 1;
    } else {
      return a.localeCompare(b);
    }
  });

  const root: TreeNode[] = [];
  const pathMap = new Map<string, TreeNode>();

  sortedFilePaths.forEach((path) => {
    const parts = path.split('/');

    let currentLevel = root;
    let currentPath = '';

    parts.forEach((part, partIndex) => {
      currentPath += (currentPath ? '/' : '') + part;
      if (part === '') {
        return;
      }

      let node = pathMap.get(currentPath);
      if (!node) {
        node = {
          id: currentPath,
          name: part,
          children: [],
        };
        pathMap.set(currentPath, node);
        currentLevel.push(node);
      }

      currentLevel = node.children!;
    });

    const parentNode = pathMap.get(currentPath);
    if (
      parentNode &&
      parentNode.children &&
      parentNode.children.length === 0 &&
      isFile(path)
    ) {
      delete parentNode.children;
    }
  });

  return root.filter((node) => node);
}

const FileViewer = () => {
  const { width, height, ref } = useResizeObserver();
  const editorFiles = useAtomValue(currentEditorFilesAtom);
  const setFiles = useSetAtom(bamlFilesTrackedAtom);
  const treeRef = useRef<TreeApi<any> | null>(null);
  const activeFile = useAtomValue(activeFileNameAtom);
  const [emptyDirs, setEmptydirs] = useAtom(emptyDirsAtom);

  const data = createTree(editorFiles.map((f) => f.path).concat(emptyDirs));

  const [term, setTerm] = useState('');

  const createFileFolder = (
    <div className="flex flex-row gap-x-1 pt-3 pl-1 w-full">
      <button
        onClick={async () => {
          await treeRef?.current?.createInternal();
        }}
        title="New Folder..."
      >
        <FolderPlus size={14} className="text-zinc-500 hover:text-zinc-200" />
      </button>
      <button
        onClick={async () => {
          const leaf = await treeRef?.current?.createLeaf();
        }}
        title="New File..."
      >
        <FilePlus size={14} className="text-zinc-500 hover:text-zinc-200" />
      </button>
    </div>
  );

  return (
    <div className="flex flex-col pl-2 w-full h-full overflow-x-clip overflow-y-hidden">
      <div className="folderFileActions">{createFileFolder}</div>
      {/* <input
        type="text"
        placeholder="Search..."
        className="search-input"
        value={term}
        onChange={(e) => setTerm(e.target.value)}
      /> */}
      <div ref={ref as React.RefCallback<HTMLDivElement>} className="flex flex-col h-full min-h-0">
        <Tree
          className="truncate"
          ref={treeRef}
          openByDefault={false}
          // initialOpenState={{ baml_src: true }}
          data={data}
          indent={12}
          initialOpenState={{ baml_src: true }}
          rowHeight={28}
          width={width}
          selection={activeFile ?? undefined}
          onMove={({ dragIds, parentId, index, dragNodes, parentNode }) => {
            if (!parentId?.includes('baml_src')) {
              return;
            }

            const emptyDirsLookup = new Set(emptyDirs);

            const emptyDirRenames = new Map<string, string>();
            const fileRenames: { from: string; to: string }[] = [];

            dragNodes.forEach((node) => {
              const stack: { nodes: NodeApi<any>[]; parents: string[] }[] = [
                {
                  nodes: [node],
                  parents: [],
                },
              ];

              while (stack.length > 0) {
                const { nodes, parents } = stack.pop()!;

                for (const node of nodes) {
                  let dest: string;

                  if (parents.length > 0) {
                    dest = `${parentId}/${parents.join('/')}/${node.id.split('/').pop() ?? ''}`;
                  } else {
                    dest = `${parentId}/${node.id.split('/').pop() ?? ''}`;
                  }

                  if (node.isLeaf) {
                    if (dest !== node.id) {
                      fileRenames.push({ from: node.id, to: dest });
                    }
                  } else {
                    if (
                      emptyDirsLookup.has(node.id) ||
                      emptyDirsLookup.has(`${node.id}/`)
                    ) {
                      emptyDirRenames.set(`${node.id}/`, `${dest}/`);
                    }
                    stack.push({
                      nodes: node.children!,
                      parents: parents.concat(node.id.split('/').pop() ?? ''),
                    });
                  }
                }
              }
            });

            console.log('onMove', { fileRenames, emptyDirRenames });

            setFiles((prev) => {
              const movedFiles = { ...prev };

              fileRenames.forEach((rename) => {
                movedFiles[rename.to] = movedFiles[rename.from] ?? '';
                delete movedFiles[rename.from];
              });

              return movedFiles;
            });

            setEmptydirs((prev) => {
              // TODO: See onRename(), there's some issue with trailing slashes, fix this.
              const movedEmptyDirs = prev.filter(
                (dir) =>
                  !emptyDirRenames.has(dir) && !emptyDirRenames.has(`${dir}/`),
              );
              emptyDirRenames.forEach((dir) => movedEmptyDirs.push(dir));

              return movedEmptyDirs;
            });
          }}
          onCreate={({ parentId, parentNode, type }) => {
            console.log('onCreate', type, parentId, parentNode);

            if (type === 'internal') {
              const newDir = `${parentId ?? 'baml_src'}/new/`;
              setEmptydirs((prev) => [...prev, newDir]);

              return { id: newDir, name: 'new_folder' };
            }

            const newFileName = `${parentId ?? 'baml_src'}/new.baml`;
            setFiles((prev) => ({ ...prev, [newFileName]: '' }));

            return { id: newFileName, name: newFileName };
          }}
          onRename={({ node, id, name }) => {
            const parentName = node.parent?.id ?? 'baml_src';
            const newName = `${parentName}/${name}`;

            const emptyDirRenames = emptyDirs
              .filter((dir) => dir.startsWith(id))
              .map((dir) => ({ from: dir, to: dir.replace(id, newName) }));

            const fileRenames = editorFiles
              .filter((f) => f.path.startsWith(id))
              .map((f) => ({ from: f.path, to: f.path.replace(id, newName) }));

            console.log('onRename', { fileRenames, emptyDirRenames });

            if (emptyDirRenames.length > 0) {
              setEmptydirs((prev) => {
                const dirs = prev.filter((dir) => !dir.startsWith(id));
                // TODO: Something's causing the last slash to be removed after
                // triggering this handler more than once. This fixes it but we
                // should find where it's removed.
                emptyDirRenames.forEach((rename) => dirs.push(`${rename.to}/`));

                console.log({ prev, dirs });

                return dirs;
              });
            }

            if (fileRenames.length > 0) {
              setFiles((prev) => {
                const newFiles = { ...prev };
                fileRenames.forEach((rename) => {
                  newFiles[rename.to] = newFiles[rename.from] ?? '';
                  delete newFiles[rename.from];
                });

                return newFiles;
              });
            }
          }}
          height={height && height > 0 ? height : 600}
          searchTerm={term}
          searchMatch={(node, term) =>
            node.data.name.toLowerCase().includes(term.toLowerCase())
          }
        >
          {Node}
        </Tree>
      </div>
    </div>
  );
};

export default FileViewer;
