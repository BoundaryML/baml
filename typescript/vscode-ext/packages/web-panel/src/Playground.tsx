import {
  VSCodeButton,
  VSCodeDropdown,
  VSCodeOption,
  VSCodePanelTab,
  VSCodePanelView,
  VSCodePanels,
  VSCodeTextArea,
} from '@vscode/webview-ui-toolkit/react'
import { Allotment } from 'allotment'
import { ParserDatabase } from '@baml/common'
import { useEffect, useMemo, useState } from 'react'
import { vscode } from './utils/vscode'
import { TestRequest } from '@baml/common'
import Ansi from 'ansi-to-react'

// window.vscode = acquireVsCodeApi()

const Playground: React.FC<{ project: ParserDatabase }> = ({ project: { functions } }) => {
  let [selectedId, setSelectedId] = useState<{
    functionName: string | undefined
    implName: string | undefined
  }>({ functionName: functions.at(0)?.name.value, implName: functions.at(0)?.impls.at(0)?.name.value })

  let { func, impl, prompt } = useMemo(() => {
    let func = functions.find((func) => func.name.value === selectedId.functionName)
    let impl = func?.impls.find((impl) => impl.name.value === selectedId.implName)

    let prompt = impl?.prompt ?? ''
    impl?.input_replacers.forEach(({ key, value }) => {
      prompt = prompt.replaceAll(key, `{${value}}`)
    })
    impl?.output_replacers.forEach(({ key, value }) => {
      prompt = prompt.replaceAll(key, value)
    })
    return { func, impl, prompt }
  }, [selectedId, functions])

  const [singleArgValue, setSingleArgValue] = useState<string>('')

  useEffect(() => {
    if (!impl && selectedId.implName !== undefined && func) {
      let implName = func.impls.at(0)?.name.value
      setSelectedId((prev) => ({ ...prev, implName }))
    }
  }, [func, impl, selectedId.implName])

  let text = `def main():
  print("Hello, world!")`

  return (
    <main className="w-full h-screen py-2">
      <div className="flex flex-row justify-between">
        <div className="justify-start">
          <VSCodeDropdown
            className="mr-1"
            value={selectedId.functionName ?? '<unset>'}
            onChange={(event) =>
              setSelectedId((prev) => ({
                ...prev,
                functionName: (event as React.FormEvent<HTMLSelectElement>).currentTarget.value,
              }))
            }
          >
            {functions.map((func, index) => (
              <VSCodeOption key={index} value={func.name.value}>
                {func.name.value}
              </VSCodeOption>
            ))}
          </VSCodeDropdown>
        </div>
        <VSCodeButton className="flex justify-end h-7">Jump to Definition</VSCodeButton>
      </div>
      {func && (
        <div className="flex flex-col">
          <span className="font-bold">Test Case</span>
          {func.input.arg_type === 'positional' ? (
            <div className="flex flex-col gap-1">
              <span className="font-bold">
                arg: <span className="font-normal">{func.input.type}</span>
              </span>
              <VSCodeTextArea
                className="w-full"
                value={singleArgValue}
                onInput={(e: any) => {
                  setSingleArgValue(e?.target?.value ?? undefined)
                }}
              />
            </div>
          ) : (
            <div className="flex flex-col gap-1">
              {func.input.values.map((value, index) => (
                <div className="flex flex-col gap-1">
                  <span className="font-bold">
                    {value.name}: <span className="font-normal">{value.type}</span>
                  </span>
                  <VSCodeTextArea className="w-full" />
                </div>
              ))}
            </div>
          )}
        </div>
      )}
      {func && (
        <VSCodePanels
          className="w-full"
          activeid={impl ? selectedId.implName : undefined}
          onChange={(e) => {
            setSelectedId((prev) => ({ ...prev, implName: (e.target as any)?.activetab?.id }))
          }}
        >
          {func.impls.map((impl, index) => (
            <>
              <VSCodePanelTab key={index} id={impl.name.value}>
                {impl.name.value}
              </VSCodePanelTab>
              <VSCodePanelView id={impl.name.value}></VSCodePanelView>
            </>
          ))}
        </VSCodePanels>
      )}
      {func && impl && (
        <>
          <div className="flex flex-col gap-1 overflow-y-scroll h-[50%]">
            <div className="flex flex-row gap-1">
              <span className="font-bold">File</span> {impl.name.source_file}
            </div>
            <div className="flex flex-row gap-1">
              <span className="font-bold">Client</span> {impl.client.value} ({impl.client.source_file})
            </div>
            <b>Prompt</b>
            <pre className="w-full p-2 whitespace-pre-wrap bg-vscode-input-background">{prompt}</pre>
          </div>
          <div className="py-3 w-fit">
            <VSCodeButton
              className="flex justify-end h-7"
              onClick={() => {
                if (!func || !impl) {
                  return
                }
                const runTestRequest: TestRequest = {
                  functions: [
                    {
                      name: func.name.value,
                      tests: [
                        {
                          name: 'random_test_name',
                          impls: [impl.name.value],
                          params: {
                            type: 'positional',
                            value: singleArgValue,
                          },
                        },
                      ],
                    },
                  ],
                }
                vscode.postMessage({
                  command: 'runTest',
                  data: runTestRequest,
                })

                // const runTestRequest:
                // vscode.postMessage({
                //   command: 'runTest',
                //   data: {},
                // })
              }}
            >
              Run
            </VSCodeButton>
          </div>
          <TestOutputBox />
        </>
      )}
    </main>
  )
}

export const TestOutputBox = () => {
  const [testOutput, setTestOutput] = useState<string>('')
  useEffect(() => {
    const fn = (event: any) => {
      const command = event.data.command
      const messageContent = event.data.content

      switch (command) {
        case 'stdout': {
          setTestOutput((prev) => (prev ? `${prev}\n${messageContent}` : messageContent))
          break
        }
        case 'reset-stdout': {
          setTestOutput('')
          break
        }
      }
    }

    // TODO: these listeners probably need to go in some seaprate provider, as we are likely losing msgs anytime this component rerenders.
    window.addEventListener('message', fn)

    return () => {
      window.removeEventListener('message', fn)
    }
  })

  return (
    <div className="flex flex-col gap-1 overflow-y-scroll h-[50%]">
      <b>Output</b>
      <pre className="w-full p-1 bg-vscode-input-background">
        <Ansi useClasses>{testOutput}</Ansi>
      </pre>
    </div>
  )
}

const FunctionPlayground: React.FC<{ func: ParserDatabase['functions'][0] }> = ({ func }) => {
  return null
}

export default Playground
