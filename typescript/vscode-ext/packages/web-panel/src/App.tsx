import React, { useEffect, useState, useRef } from 'react'
import { vscode } from './utils/vscode'
import {
  VSCodeButton,
  VSCodeTextArea,
  VSCodeDropdown,
  VSCodeOption,
  VSCodeDivider,
} from '@vscode/webview-ui-toolkit/react'
import { Allotment } from 'allotment'
import 'allotment/dist/style.css'

import './App.css'
import { TextArea } from '@vscode/webview-ui-toolkit'

function App() {
  // function handleHowdyClick() {
  //   vscode.postMessage({
  //     command: 'hello',
  //     text: 'Hey there partner! 🤠',
  //   })
  // }

  const [payload, setPayload] = useState<string>()
  const [text, setText] = useState<string>('')
  const [functions, setFunctions] = useState<string[]>(['func 1', 'func 2'])
  const [variants, setVariants] = useState<string[]>(['impl 1', 'impl 2'])

  useEffect(() => {
    const fn = (event: any) => {
      const command = event.data.command
      const messageContent = event.data.content
      console.log(JSON.stringify(messageContent, null, 2))

      switch (command) {
        case 'sendInputSchema':
          setPayload(messageContent)
          setText(JSON.stringify(messageContent, null, 2))
          break
      }
    }

    window.addEventListener('message', fn)

    return () => {
      window.removeEventListener('message', fn)
    }
  }, [])

  return (
    <main className="h-[500px] min-w-[500px]">
      <div className="flex flex-row justify-between p-2">
        <div className="justify-start">
          <VSCodeDropdown className="mr-1">
            {functions.map((func, index) => (
              <VSCodeOption key={index} value={func}>
                {func}
              </VSCodeOption>
            ))}
          </VSCodeDropdown>
          <VSCodeDropdown>
            {variants.map((variant, index) => (
              <VSCodeOption key={index} value={variant}>
                {variant}
              </VSCodeOption>
            ))}
          </VSCodeDropdown>
        </div>
        <VSCodeButton className="flex justify-end h-7">Jump to Definition</VSCodeButton>
      </div>
      <hr />
      <Allotment>
        <Allotment.Pane minSize={300} className="p-2">
          <div className="flex flex-col w-100">
            <VSCodeTextArea
              value={text}
              className="w-100"
              rows={Math.min(text.split('\n').length + 1, 20)}
              onInput={(event) => {
                // setText(event.target?.value)
                // console.log(event.target?.value.split('\n').length + 1)
              }}
            >
              Input
            </VSCodeTextArea>
            <VSCodeTextArea className="mt-1 w-100" readOnly>
              Rendered Prompt
            </VSCodeTextArea>
          </div>
        </Allotment.Pane>
        <Allotment.Pane minSize={100} className="p-2">
          <div className="flex flex-col">
            <div className="flex flex-row justify-end w-100">
              <VSCodeButton className="w-32">Run</VSCodeButton>
            </div>
            <VSCodeTextArea className="mt-1 w-100" readOnly>
              Output
            </VSCodeTextArea>
          </div>
        </Allotment.Pane>
      </Allotment>
    </main>
  )
}

export default App
