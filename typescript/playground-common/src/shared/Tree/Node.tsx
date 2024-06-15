import clsx from 'clsx'
import { useSetAtom } from 'jotai'
import { ChevronDown, ChevronRight, File, Folder } from 'lucide-react'
import { useEffect } from 'react'
import type { NodeRendererProps } from 'react-arborist'
import { SiJinja } from 'react-icons/si'
import { activeFileAtom } from './atoms'

const renderIcon = (path: string) => {
  const icon = path.split('.').pop()
  switch (icon) {
    case 'jinja':
      return <SiJinja size={16} color='#6bc7f6' className="node-icon" />
    default:
      return (
        <span className='file-folder-icon'>
          <File color='#6bc7f6' size={18} className="node-icon" />
        </span>
      )
  }
}

const Node = ({ node, style, dragHandle }: NodeRendererProps<any>) => {
  const setActiveFile = useSetAtom(activeFileAtom)


  useEffect(() => {
    if (node.isSelected && (!node.children || node.children.length === 0)) {
      setActiveFile(node.id)
    }
  }, [node.isSelected])

  return (
    <div
      className={clsx(
        'node-container group relative cursor-pointer overflow-x-clip flex flex-col',
        node.state.isSelected ? 'bg-zinc-600 text-white' : 'text-muted-foreground'
      )}
      style={style}
      ref={dragHandle}
    >
      <div className='flex flex-row items-center w-full gap-x-2' onClick={() => node.isInternal && node.toggle()}>
        <span className='arrow'>{node.isLeaf ? null : (node.isOpen ? <ChevronDown size={16} /> : <ChevronRight size={16} />)}</span>
        <span className='node-text'>
          {/* {renderIcon(node.id)} */}
          {node.data.name}
        </span>
      </div>
    </div>
  )
}

export default Node
