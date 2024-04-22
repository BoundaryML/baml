// 1: Uncontrolled Tree
import { useEffect, useRef, useState } from 'react'

import { RenameHandler, Tree, TreeApi } from 'react-arborist'

import Node from './Node'
import { FilePlus, FolderPlus } from 'lucide-react'
import useResizeObserver from 'use-resize-observer'
import { useAtom, useAtomValue } from 'jotai'
import { activeFileAtom, currentEditorFilesAtom } from '../../_atoms/atoms'
import { EditorFile } from '@/app/actions'

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

    let parentNode = pathMap.get(currentPath)
    if (parentNode && parentNode.children && parentNode.children.length === 0) {
      delete parentNode.children
    }
  })

  return root.filter((node) => node)
}

const FileViewer = () => {
  const { width, height, ref } = useResizeObserver()
  const [editorFiles, setEditorFiles] = useAtom(currentEditorFilesAtom)
  const treeRef = useRef<TreeApi<any> | null>(null)
  const activeFile = useAtomValue(activeFileAtom)

  const data2 = createTree(editorFiles.map((f) => f.path))

  const [term, setTerm] = useState('')

  const createFileFolder = (
    <div className="flex flex-row w-full pt-3 pl-1 gap-x-1">
      {/* <button
        onClick={() => {
          treeRef?.current?.createInternal()
        }}
        title="New Folder..."
      >
        <FolderPlus size={14} className="text-zinc-500 hover:text-zinc-200" />
      </button> */}
      <button
        onClick={async () => {
          console.log('leaf', treeRef?.current)

          const leaf = await treeRef?.current?.createLeaf()
        }}
        title="New File..."
      >
        <FilePlus size={14} className="text-zinc-500 hover:text-zinc-200" />
      </button>
    </div>
  )

  return (
    <div className="overflow-x-clip">
      <div className="folderFileActions">{createFileFolder}</div>
      {/* <input
        type="text"
        placeholder="Search..."
        className="search-input"
        value={term}
        onChange={(e) => setTerm(e.target.value)}
      /> */}
      <aside ref={ref} className="">
        <Tree
          className="truncate"
          ref={treeRef}
          openByDefault={false}
          initialOpenState={{ baml_src: true }}
          data={data2}
          // initialOpenState={{ baml_src: true }}
          rowHeight={24}
          width={width}
          selection={activeFile?.path}
          onCreate={({ parentId, parentNode }) => {
            console.log('onCreate', parentId, parentNode)
            const newFileName = 'new'
            setEditorFiles((prev) => {
              prev = prev as EditorFile[]
              return [...prev, { path: `baml_src/${newFileName}`, content: '' }]
            })
            return { id: `baml_src/${newFileName}`, name: newFileName }
          }}
          height={200}
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
