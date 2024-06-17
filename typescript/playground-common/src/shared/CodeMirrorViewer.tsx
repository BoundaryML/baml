'use client'
import React from 'react';
import { BAML, theme } from '@baml/codemirror-lang'
import CodeMirror, {EditorView} from '@uiw/react-codemirror'


const extensions = [BAML(), EditorView.lineWrapping]

export const CodeMirrorViewer = ({ fileContent }: { fileContent: string }) => {
  return (
    <div className='w-full'>
      <div
        className='relative'
        style={{
          height: '100%',
        }}
      >
        <CodeMirror
          value={fileContent}
          extensions={extensions}
          theme={theme}
          readOnly={true}
          className='text-xs lg:text-sm'
          height='100%'
          width='100%'
          maxWidth='100%'
          style={{ width: '100%', height: '100%' }}
        />
      </div>
    </div>
  )
}
