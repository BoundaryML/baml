import { EditorFile } from '@/app/actions'
import { diagnositicsAtom, updateFileAtom } from '@baml/playground-common/baml_wasm_web/EventListener'
import { runtimeFamilyAtom } from '@baml/playground-common/baml_wasm_web/baseAtoms'
import clsx from 'clsx'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { useAtomCallback } from 'jotai/utils'
import { ArrowDown, ArrowRight, ChevronDown, ChevronRight, Edit, Edit2, File, Folder, X } from 'lucide-react'
import { useEffect, useMemo } from 'react'
import type { NodeRendererProps } from 'react-arborist'
import { SiPython, SiTypescript } from 'react-icons/si'
import { PROJECT_ROOT, activeFileNameAtom, currentEditorFilesAtom, emptyDirsAtom } from '../../_atoms/atoms'

export type Entity = {
  id: string
  name: string
  fullPath: string
  object: any
}

const renderIcon = (path: string) => {
  const icon = path.split('.').pop()
  switch (icon) {
    case 'py':
      return <SiPython size={14} color='#6bc7f6' />
    case 'ts':
      return <SiTypescript size={14} color='#2563eb' />
    default:
      return (
        <span className='file-folder-icon'>
          <File color='#6bc7f6' size={16} />
        </span>
      )
  }
}

const Node = ({ node, style, dragHandle, tree }: NodeRendererProps<any>) => {
  const CustomIcon = node.data.icon
  const iconColor = node.data.iconColor
  const editorFiles = useAtomValue(currentEditorFilesAtom)
  const setActiveFile = useSetAtom(activeFileNameAtom)
  const updateFile = useSetAtom(updateFileAtom)

  const hasErrorInChildren = useAtomCallback<boolean, string[]>((get, _set, nodeId: string) => {
    const nodes = [tree.get(nodeId)] // Start with the current node

    const diagnosticErrors = get(diagnositicsAtom)
    const errors = diagnosticErrors.filter((d) => d.type === 'error')
    while (nodes.length > 0) {
      const currentNode = nodes.pop()
      if (currentNode?.children) {
        currentNode.children.forEach((child) => {
          nodes.push(tree.get(child.id))
        })
      }
      if (errors.some((d) => d.file_path === currentNode?.id)) {
        return true
      }
    }
    return false
  })

  const setEmptyDirs = useSetAtom(emptyDirsAtom)

  useEffect(() => {
    if (node.isSelected) {
      setActiveFile(node.id)
    }
  }, [node.isSelected])

  // Check if the current file or any children have errors
  const fileHasErrors = hasErrorInChildren(node.id)

  return (
    <div
      className={clsx(
        `group relative px-2 py-1 cursor-pointer overflow-x-clip flex-flex-col text-xs ${
          node.state.isSelected ? 'isSelected' : ''
        }`,
        [node.state.isSelected ? 'bg-zinc-600' : ''],
      )}
      style={style}
      ref={dragHandle}
    >
      <div className='flex flex-row items-center w-full pl-2 gap-x-1' onClick={() => node.isInternal && node.toggle()}>
        {node.isLeaf ? (
          <>
            <span className='arrow'></span>
            {renderIcon(node.id)}
          </>
        ) : (
          <>
            <span className='arrow'>{node.isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</span>
            {/* <span className="file-folder-icon">
              <Folder color="#f6cf60" size={16} />
            </span> */}
          </>
        )}
        <span className='node-text text-muted-foreground'>
          {node.isEditing ? (
            <input
              type='text'
              defaultValue={node.data.name}
              onFocus={(e) => e.currentTarget.select()}
              onBlur={() => node.reset()}
              onKeyDown={(e) => {
                if (e.key === 'Escape') node.reset()
                if (e.key === 'Enter') {
                  // Previous name folder name
                  node.submit(e.currentTarget.value)
                  const filePathWithNoFilename = node.id.split('/').slice(0, -1).join('/')
                  const fileName = `${filePathWithNoFilename}/${e.currentTarget.value}`
                  updateFile({
                    reason: 'rename_file',
                    root_path: PROJECT_ROOT,
                    files: [],
                    renames: [{ from: node.id, to: fileName }],
                  })

                  setEmptyDirs((prev) => {
                    prev = prev as string[]
                    return prev.map((d) => {
                      d = d.slice(0, -1)
                      if (d === node.id) {
                        const dirPathWithNoDirname = d.split('/').slice(0, -1).join('/')
                        return `${dirPathWithNoDirname}/${e.currentTarget.value}`
                      }
                      return d
                    })
                  })
                }
              }}
              autoFocus
            />
          ) : (
            <span className={clsx(fileHasErrors ? 'text-red-500' : node.state.isSelected ? 'text-white' : '')}>
              {node.data.name}
            </span>
          )}
        </span>
      </div>

      {node.id !== 'baml_src' && (
        <div className='absolute top-0 right-0 hidden rounded-md group-hover:flex bg-zinc-800'>
          <div className='flex flex-row items-center gap-x-1 '>
            <button
              className='p-1 hover:opacity-100 opacity-70'
              onClick={(e) => {
                e.stopPropagation()
                node.edit()
              }}
              title='Rename...'
            >
              <Edit2 size={11} />
            </button>
            <button
              className='p-1 hover:opacity-100 opacity-60'
              onClick={() => {
                tree.delete(node.id)

                updateFile({
                  reason: 'delete_file',
                  root_path: PROJECT_ROOT,
                  files: [
                    {
                      name: node.id,
                      content: undefined,
                    },
                  ],
                })
                setEmptyDirs((prev) => {
                  prev = prev as string[]
                  return prev.filter((d) => d.slice(0, -1) !== node.id)
                })
              }}
              title='Delete'
            >
              <X size={16} />
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

export default Node
