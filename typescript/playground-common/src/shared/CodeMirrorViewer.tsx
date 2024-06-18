'use client'
import React from 'react';
import { BAML, theme} from '@baml/codemirror-lang'
import {python} from '@codemirror/lang-python'
import {javascript} from '@codemirror/lang-javascript'

import CodeMirror, {EditorView} from '@uiw/react-codemirror'



export const CodeMirrorViewer = ({ fileContent, lang}: { fileContent: string; lang:string }) => {
  let extensions = [EditorView.lineWrapping]


  if (lang === 'python'){
    extensions.push(python());
  }
  else if (lang === 'javascript'){
    extensions.push(javascript());
  }
  else {
    extensions.push(BAML());
  }

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
