import clsx from 'clsx'
import { useAtom, useAtomValue, useSetAtom } from 'jotai'
import { ArrowDown, ArrowRight, ChevronDown, ChevronRight, Edit, Edit2, File, Folder, X } from 'lucide-react'
import { NodeRendererProps } from 'react-arborist'
import { activeFileAtom, currentEditorFilesAtom, fileDiagnostics } from '../../_atoms/atoms'
import { EditorFile } from '@/app/actions'
import { useEffect } from 'react'

export type Entity = {
  id: string
  name: string
  fullPath: string
  object: any
}

const Node = ({ node, style, dragHandle, tree }: NodeRendererProps<any>) => {
  const CustomIcon = node.data.icon
  const iconColor = node.data.iconColor
  const [editorFiles, setEditorFiles] = useAtom(currentEditorFilesAtom)
  const setActiveFile = useSetAtom(activeFileAtom)
  const diagnostics = useAtomValue(fileDiagnostics)

  useEffect(() => {
    if (node.isSelected) {
      const editorFile = editorFiles.find((f) => f.path === node.id)
      if (!editorFile) return
      setActiveFile(editorFile)
    }
  }, [node.isSelected, editorFiles])

  const hasErrorInChildren = (nodeId: string) => {
    const nodes = [tree.get(nodeId)] // Start with the current node
    while (nodes.length > 0) {
      const currentNode = nodes.pop()
      if (currentNode?.children) {
        currentNode.children.forEach((child) => {
          nodes.push(tree.get(child.id))
        })
      }
      if (diagnostics.some((d) => d.source === currentNode?.id)) {
        return true
      }
    }
    return false
  }

  // Check if the current file or any children have errors
  const fileHasErrors = diagnostics.some((d) => d.source === node.id) || hasErrorInChildren(node.id)

  return (
    <div
      className={clsx(
        `group relative px-2 py-1 cursor-pointer  flex-flex-col text-xs ${node.state.isSelected ? 'isSelected' : ''}`,
        [node.state.isSelected ? 'bg-zinc-600' : ''],
      )}
      style={style}
      ref={dragHandle}
    >
      <div className="flex flex-row items-center w-full gap-x-1" onClick={() => node.isInternal && node.toggle()}>
        {node.isLeaf ? (
          <>
            <span className="arrow"></span>
            <span className="file-folder-icon">
              <File color="#6bc7f6" size={16} />
            </span>
          </>
        ) : (
          <>
            <span className="arrow">{node.isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />}</span>
            {/* <span className="file-folder-icon">
              <Folder color="#f6cf60" size={16} />
            </span> */}
          </>
        )}
        <span className="node-text text-muted-foreground">
          {node.isEditing ? (
            <input
              type="text"
              defaultValue={node.data.name}
              onFocus={(e) => e.currentTarget.select()}
              onBlur={() => node.reset()}
              onKeyDown={(e) => {
                if (e.key === 'Escape') node.reset()
                if (e.key === 'Enter') {
                  node.submit(e.currentTarget.value)
                  setEditorFiles((prev) => {
                    prev = prev as EditorFile[]
                    return prev.map((f) => {
                      if (f.path === node.id) {
                        const filePathWithNoFilename = f.path.split('/').slice(0, -1).join('/')
                        return { ...f, path: `${filePathWithNoFilename}/${e.currentTarget.value}` }
                      }
                      return f
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

      <div className="absolute top-0 right-0 hidden group-hover:flex">
        <div className="flex flex-row items-center gap-x-1 ">
          <button
            className="p-1 hover:opacity-100 opacity-80"
            onClick={(e) => {
              e.stopPropagation()
              node.edit()
            }}
            title="Rename..."
          >
            <Edit2 size={11} />
          </button>
          <button className="p-1 hover:opacity-100 opacity-80" onClick={() => tree.delete(node.id)} title="Delete">
            <X size={16} />
          </button>
        </div>
      </div>
    </div>
  )
}

export default Node
