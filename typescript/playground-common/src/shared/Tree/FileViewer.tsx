import React from 'react';
import { Tree, type TreeApi } from 'react-arborist';
import Node from './Node';


export const data = [
  {
    id: 'jinja_dir',
    name: 'jinja',
    children: [{ id: 'jinja_prompts', name: 'jinja_prompts' }],
  },
  {
    id: 'clients_dir',
    name: 'clients',
    children: [
      { id: 'dynamic_clients', name: 'dynamic_clients' },
      { id: 'c2-2', name: 'index.js' },
      { id: 'c2-3', name: 'styles.css' },
    ],
  },
  // { id: '3', name: 'package.json' },
  // { id: '4', name: 'README.md' },
];

export const FileViewer = () => {
  const treeRef = React.useRef<TreeApi<any> | null>(null);

  return (
    <div className='flex flex-col w-full h-full overflow-hidden'>
      <Tree
        ref={treeRef}
        openByDefault={true}
        data={data}
        rowHeight={60} // Increased from 24 to 32 for better spacing
        className='tree-container' // Custom class for further styling
      >
        {Node}
      </Tree>
    </div>
  );
}

export default FileViewer;
