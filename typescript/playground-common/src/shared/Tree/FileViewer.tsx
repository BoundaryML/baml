import React from 'react';
import { Tree, type TreeApi } from 'react-arborist';
import Node from './Node';


export const data = [
  {
    id: 'prompt-engineering', 
    icon: 'star',
    name: 'Prompt engineering',
    children: [{ id: 'system_user_prompts', name: 'Prompt roles' }],
  },
  {
    id: 'testing',
    icon: 'beakers',
    name: 'Testing',
    children: [
      { id: 'test_ai_function', name: 'Test an AI function' },
      { id: 'evaluate_results', name: 'Evaluate results' },
    ],
  },
  {
    id: 'resilience_reliability',
    icon: 'shield',
    name: 'Resilence / Reliability',
    children: [
      { id: 'add_retries', name: 'Function retries' },
      { id: 'fall_back', name: 'Model fall-back' },
    ],
  },
  {
    id: 'observability',
    icon: 'graph',
    name: 'Observability',
    children: [
      { id: 'tracing_tagging', name: 'Tracing functions' }
      
    ],
  },
  {
    id: 'improve_llm_results',
    icon: 'lightning-bolt',
    name: 'Improve LLM results',
    children: [
      { id: 'improve_prompt_auto', name: 'Auto-Improve prompt' },
      { id: 'fine-tune', name: 'Fine-tune a model' },
    ],
  },
  {
    id: 'streaming_dir',
    icon: 'waves',
    name: 'Streaming',
    children: [
      { id: 'streaming_structured', name: 'Structured streaming' },
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
        rowHeight={32} // Increased from 24 to 32 for better spacing
        className='tree-container' // Custom class for further styling
      >
        {Node}
      </Tree>
    </div>
  );
}

export default FileViewer;
