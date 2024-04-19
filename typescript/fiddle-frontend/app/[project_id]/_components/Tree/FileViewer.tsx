// 1: Uncontrolled Tree
import { useRef, useState } from 'react'

import { Tree, TreeApi } from 'react-arborist'

import Node from './Node'
import { FilePlus, FolderPlus } from 'lucide-react'
import useResizeObserver from 'use-resize-observer'
import { useAtom } from 'jotai'
import { currentEditorFilesAtom } from '../../_atoms/atoms'

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
]

interface TreeNode {
  id: string
  name: string
  children?: TreeNode[]
}

function createTree(filePaths: string[]): TreeNode[] {
  const root: TreeNode[] = []
  const pathMap = new Map<string, TreeNode>()

  filePaths.forEach((path) => {
    const parts = path.split('/')

    let currentLevel = root
    let currentPath = ''

    parts.forEach((part, partIndex) => {
      currentPath += (currentPath ? '/' : '') + part

      let node = pathMap.get(currentPath)
      if (!node) {
        node = {
          id: currentPath,
          name: part,
          children: [],
        }
        pathMap.set(currentPath, node)
        currentLevel.push(node)
      }

      currentLevel = node.children!
    })
  })

  return root.filter((node) => node) // Filter out undefined entries, if any.
}

const FileViewer = () => {
  const { width, height, ref } = useResizeObserver()
  const editorFiles = useAtom(currentEditorFilesAtom)
  const treeRef = useRef<TreeApi<any> | null>(null)

  // const { width, height, ref } = useResizeObserver({ ref: treeRef as any })
  const [term, setTerm] = useState('')

  const createFileFolder = (
    <>
      <button onClick={() => treeRef?.current?.createInternal()} title="New Folder...">
        <FolderPlus />
      </button>
      <button onClick={() => treeRef?.current?.createLeaf()} title="New File...">
        <FilePlus />
      </button>
    </>
  )

  return (
    <div className="">
      <div className="folderFileActions">{createFileFolder}</div>
      <input
        type="text"
        placeholder="Search..."
        className="search-input"
        value={term}
        onChange={(e) => setTerm(e.target.value)}
      />
      <aside ref={ref}>
        <Tree
          ref={treeRef}
          initialData={data}
          // indent={24}
          rowHeight={24}
          // openByDefault={false}
          width={width}
          height={height}
          searchTerm={term}
          searchMatch={(node, term) => node.data.name.toLowerCase().includes(term.toLowerCase())}
        >
          {Node}
        </Tree>
      </aside>
    </div>
  )
}

export default FileViewer
