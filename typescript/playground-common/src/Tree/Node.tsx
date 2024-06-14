import clsx from 'clsx'
import { useSetAtom } from 'jotai'
import { ChevronDown, ChevronRight, File, Folder } from 'lucide-react'
import { useEffect } from 'react'
import type { NodeRendererProps } from 'react-arborist'
import { SiJinja } from 'react-icons/si'
import { activeFileAtom } from './atoms'

export type Entity = {
  id: string
  name: string
  fullPath: string
  object: any
}

const renderIcon = (path: string) => {
  const icon = path.split('.').pop()
  switch (icon) {
    case 'jinja':
      return <SiJinja size={14} color='#6bc7f6' />
    default:
      return (
        <span className='file-folder-icon'>
          <File color='#6bc7f6' size={16} />
        </span>
      )
  }
}

const Node = ({ node, style, dragHandle }: NodeRendererProps<any>) => {
  const setActiveFile = useSetAtom(activeFileAtom)

  useEffect(() => {
    if (node.isSelected) {
      setActiveFile(node.id)
    }
  }, [node.isSelected])

  return (
    <div
      className={clsx(
        'group relative px-2 py-1 cursor-pointer overflow-x-clip flex flex-col text-xs',
        node.state.isSelected ? 'bg-zinc-600 text-white' : 'text-muted-foreground'
      )}
      style={style}
      ref={dragHandle}
    >
      <div className='flex flex-row items-center w-full pl-2 gap-x-1' onClick={() => node.isInternal && node.toggle()}>
        <span className='arrow'>{node.isLeaf ? null : (node.isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />)}</span>
        <span className='node-text'>
          {renderIcon(node.id)}
          {node.data.name}
        </span>
      </div>
    </div>
  )
}

export default Node