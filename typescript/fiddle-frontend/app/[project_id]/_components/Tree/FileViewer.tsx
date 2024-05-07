// 1: Uncontrolled Tree
import { useEffect, useRef, useState } from 'react'

import { MoveHandler, RenameHandler, Tree, type TreeApi } from 'react-arborist'

import { EditorFile } from '@/app/actions'
import { updateFileAtom } from '@baml/playground-common/baml_wasm_web/EventListener'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { FilePlus, FolderPlus } from 'lucide-react'
import useResizeObserver from 'use-resize-observer'
import { PROJECT_ROOT, activeFileNameAtom, currentEditorFilesAtom, emptyDirsAtom } from '../../_atoms/atoms'
import Node from './Node'

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
const isFile = (path: string) => path.includes('.')

function createTree(filePaths: string[]): TreeNode[] {
  // Sort paths folders first, then files, alphabetically.
  const sortedFilePaths = filePaths.sort((a, b) => {
    const isAFolder = !isFile(a)
    const isBFolder = !isFile(b)

    if (isAFolder && !isBFolder) {
      return -1
    } else if (!isAFolder && isBFolder) {
      return 1
    } else {
      return a.localeCompare(b)
    }
  })

  const root: TreeNode[] = []
  const pathMap = new Map<string, TreeNode>()

  sortedFilePaths.forEach((path) => {
    const parts = path.split('/')

    let currentLevel = root
    let currentPath = ''

    parts.forEach((part, partIndex) => {
      currentPath += (currentPath ? '/' : '') + part
      if (part === '') {
        return
      }

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

    const parentNode = pathMap.get(currentPath)
    if (parentNode && parentNode.children && parentNode.children.length === 0) {
      if (isFile(path)) {
        delete parentNode.children
      }
    }
  })

  return root.filter((node) => node)
}

const FileViewer = () => {
  const { width, height = 200, ref } = useResizeObserver()
  const editorFiles = useAtomValue(currentEditorFilesAtom)
  const updateFile = useSetAtom(updateFileAtom)
  const treeRef = useRef<TreeApi<any> | null>(null)
  const activeFile = useAtomValue(activeFileNameAtom)
  const [emptyDirs, setEmptydirs] = useAtom(emptyDirsAtom)

  const data2 = createTree(editorFiles.map((f) => f.path).concat(emptyDirs))

  const [term, setTerm] = useState('')

  const createFileFolder = (
    <div className='flex flex-row w-full pt-3 pl-1 gap-x-1'>
      <button
        onClick={async () => {
          await treeRef?.current?.createInternal()
        }}
        title='New Folder...'
      >
        <FolderPlus size={14} className='text-zinc-500 hover:text-zinc-200' />
      </button>
      <button
        onClick={async () => {
          const leaf = await treeRef?.current?.createLeaf()
        }}
        title='New File...'
      >
        <FilePlus size={14} className='text-zinc-500 hover:text-zinc-200' />
      </button>
    </div>
  )

  return (
    <div className='flex flex-col w-full h-full overflow-x-clip'>
      <div className='pl-2 folderFileActions'>{createFileFolder}</div>
      {/* <input
        type="text"
        placeholder="Search..."
        className="search-input"
        value={term}
        onChange={(e) => setTerm(e.target.value)}
      /> */}
      <div ref={ref} className='flex flex-col h-full '>
        <Tree
          className='truncate '
          ref={treeRef}
          openByDefault={false}
          // initialOpenState={{ baml_src: true }}
          data={data2}
          initialOpenState={{ baml_src: true }}
          rowHeight={24}
          width={width}
          selection={activeFile ?? undefined}
          onMove={({ dragIds, parentId, index, dragNodes, parentNode }) => {
            if (!parentId?.includes('baml_src')) {
              return
            }

            const renames = dragIds
              .filter((id) => id.endsWith('.baml') || id.endsWith('.json'))
              .map((id) => ({
                from: id,
                to: `${parentId}/${id.split('/').pop() ?? ''}`,
              }))

            updateFile({
              reason: 'move_files',
              root_path: PROJECT_ROOT,
              files: [],
              renames,
            })
          }}
          onCreate={({ parentId, parentNode, type }) => {
            if (type === 'internal') {
              const newDir = `${parentId ?? 'baml_src'}/new/`
              setEmptydirs((prev) => [...prev, newDir])

              return { id: newDir, name: 'new_folder' }
            }
            console.log('onCreate', parentId, parentNode)

            const newFileName = 'new.baml'
            updateFile({
              reason: 'create_file',
              root_path: PROJECT_ROOT,
              files: [
                {
                  name: `baml_src/${newFileName}`,
                  content: '',
                },
              ],
            })

            return { id: `baml_src/${newFileName}`, name: newFileName }
          }}
          height={height}
          searchTerm={term}
          searchMatch={(node, term) => node.data.name.toLowerCase().includes(term.toLowerCase())}
        >
          {Node}
        </Tree>
      </div>
    </div>
  )
}

export default FileViewer
