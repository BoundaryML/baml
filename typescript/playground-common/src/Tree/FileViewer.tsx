import React from 'react'
import { Tree, type TreeApi } from 'react-arborist'
import Node from './Node'

export const data = [
  {
    id: '1',
    name: 'public',
    children: [{ id: 'c1-1', name: 'index.html' }],
  },
  {
    id: '2',
    name: 'src',
    children: [
      { id: 'c2-1', name: 'App.js' },
      { id: 'c2-2', name: 'index.js' },
      { id: 'c2-3', name: 'styles.css' },
    ],
  },
  { id: '3', name: 'package.json' },
  { id: '4', name: 'README.md' },
]

export const FileViewer = () => {
  const treeRef = React.useRef<TreeApi<any> | null>(null)

  return (
    <div className='flex flex-col w-full h-full overflow-hidden'>
      <Tree ref={treeRef} openByDefault={true} data={data} rowHeight={24} className='truncate'>
        {Node}
      </Tree>
    </div>
  )
}

export default FileViewer
