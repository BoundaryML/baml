/// Content once a function has been selected.

import type { UiSchema } from '@rjsf/utils'

import { type ParserDatabase, type StringSpan, TestRequest } from '@baml/common'
import type { WasmTestCase } from '@gloo-ai/baml-schema-wasm-web/baml_schema_build'
import Form, { getDefaultRegistry } from '@rjsf/core'
import validator from '@rjsf/validator-ajv8'
import { VSCodeButton, VSCodeProgressRing, VSCodeTextArea, VSCodeTextField } from '@vscode/webview-ui-toolkit/react'
import { useAtom, useAtomValue } from 'jotai'
import { JSONSchemaFaker as jsf } from 'json-schema-faker'
import { Copy, Edit2, FileJson2, Pin, Play, PlusIcon, Save, Trash2 } from 'lucide-react'
import type React from 'react'
import { ChangeEvent, FocusEvent, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import JsonView from 'react18-json-view'
import { Config, adjectives, animals, colors, uniqueNamesGenerator } from 'unique-names-generator'
import { selectedFunctionAtom, selectedTestCaseAtom } from '../baml_wasm_web/EventListener'
import { useRunHooks } from '../baml_wasm_web/test_uis/testHooks'
import { Badge } from '../components/ui/badge'
import { Button } from '../components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '../components/ui/dialog'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../components/ui/tooltip'
import { vscode } from '../utils/vscode'
import { ASTContext } from './ASTProvider'
import { TEMPLATES } from './TestCaseEditor/JsonEditorTemplates'
import TypeComponent from './TypeComponent'

const uiSchema: UiSchema = {
  'ui:submitButtonOptions': {
    submitText: 'Save',
    props: {
      className: 'bg-vscode-button-background px-2',
    },
  },
  'ui:autocomplete': 'on',
  'ui:options': {
    removable: true,
  },
}

type Func = ParserDatabase['functions'][number]
type TestCase = Func['test_cases'][number]

const TestCasePanelEntry: React.FC<{ test_case: WasmTestCase }> = ({ test_case }) => {
  const { run, isRunning } = useRunHooks()
  return (
    <div key={test_case.name} className='flex flex-col py-1 pr-2 w-full overflow-x-clip gap-1 group'>
      <div className='flex flex-row gap-1 items-center'>
        <Badge
          className='bg-vscode-editorSuggestWidget-selectedBackground text-vscode-editorSuggestWidget-foreground hover:bg-vscode-editorSuggestWidget-selectedBackground'
          variant='default'
        >
          <div className='flex flex-row gap-x-1 items-center'>
            <Pin size={12} /> Pinned
          </div>
        </Badge>
        <div>
          <b>Test Case: </b>
          {test_case.name}
        </div>
      </div>
      <div className='flex flex-row justify-between items-center'>
        <div className='flex flex-row gap-x-1 justify-center items-center'>
          <Button
            variant={'ghost'}
            size={'icon'}
            className='p-1 rounded-md w-fit h-fit bg-vscode-button-background text-vscode-button-foreground hover:bg-vscode-button-hoverBackground text-xs'
            disabled={isRunning}
            onClick={() => {
              run([test_case.name])
            }}
          >
            <div className='flex flex-row gap-1 items-center'>
              <Play size={10} /> <span className='hidden group-hover:flex'>Run</span>
            </div>
          </Button>
          <div className='flex flex-nowrap w-full'>
            <div className='hidden gap-x-1 group-hover:flex'>
              <Tooltip delayDuration={100}>
                <TooltipTrigger asChild>
                  <Button
                    variant={'ghost'}
                    size={'icon'}
                    className='p-1 w-fit h-fit text-vscode-descriptionForeground hover:bg-vscode-button-secondaryHoverBackground'
                    onClick={() => {
                      vscode.postMessage({ command: 'jumpToFile', data: test_case.name })
                    }}
                  >
                    <FileJson2 size={14} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent className='flex flex-col gap-y-1'>Open test file</TooltipContent>
              </Tooltip>
              {/* <Tooltip delayDuration={100}>
                <TooltipTrigger>
                  <EditTestCaseForm
                    testCase={test_case}
                    schema={input_json_schema}
                    func={func}
                    getTestParams={(t) => getTestParams(func, t)}
                    duplicate
                  >
                    <Button
                      variant={'ghost'}
                      size="icon"
                      className="p-1 w-fit h-fit hover:bg-vscode-button-secondaryHoverBackground"
                    >
                      <Copy size={12} />
                    </Button>
                  </EditTestCaseForm>
                </TooltipTrigger>
                <TooltipContent className="flex flex-col gap-y-1">Duplicate</TooltipContent>
              </Tooltip> */}
            </div>
          </div>
        </div>
        <Button
          variant={'ghost'}
          size={'icon'}
          className='p-1 w-fit h-fit text-vscode-input-foreground'
          onClick={() => {
            // vscode.postMessage({
            //   command: 'removeTest',
            //   data: {
            //     root_path,
            //     funcName: func.name.value,
            //     testCaseName: test_case.name,
            //   },
            // })
          }}
        >
          <Trash2 size={10} />
        </Button>
      </div>
      <TestCaseCard test_case={test_case} isRendered />
      {/* <EditTestCaseForm
        testCase={test_case}
        schema={input_json_schema}
        func={func}
        getTestParams={(t) => getTestParams(func, t)}
      >
        <Button
          variant={'ghost'}
          className="justify-start items-start px-1 py-1 w-full text-left hover:bg-vscode-button-secondaryHoverBackground h-fit"
        >
          
        </Button>
      </EditTestCaseForm> */}
    </div>
  )
}

const getTestParams = (func: Func, testCase: Func['test_cases'][number]) => {
  if (func.input.arg_type === 'positional') {
    return {
      type: 'positional',
      value: testCase.content,
    }
  } else {
    // sort of a hack, means the test file was just initialized since input: null is the default
    if (testCase.content === 'null') {
      return {
        type: 'named',
        value: func.input.values.map(({ name }) => ({
          name: name.value,
          value: null,
        })),
      }
    }
    const parsed = JSON.parse(testCase.content)
    let contentMap = new Map<string, string>()
    if (typeof parsed === 'object') {
      contentMap = new Map(
        Object.entries(parsed).map(([k, v]) => {
          if (typeof v === 'string') return [k, v]
          return [k, JSON.stringify(v, null, 2)]
        }),
      )
    }
    return {
      type: 'named',
      value: func.input.values.map(({ name }: { name: StringSpan }) => ({
        name: name.value,
        value: contentMap.get(name.value) ?? null,
      })),
    }
  }
}

const autoGenTestCase = (func: Func, input_json_schema: any): TestCase => {
  return {
    name: {
      ...func.name,
      value: uniqueNamesGenerator({
        dictionaries: [adjectives, colors, animals],
        separator: '_',
        length: 2,
      }),
    },
    content: JSON.stringify(jsf.generate(input_json_schema)),
  }
}

const TestCasePanel: React.FC = () => {
  const selectedTestCase = useAtomValue(selectedTestCaseAtom)

  if (selectedTestCase === null) {
    return <div className='flex flex-col w-full h-full'></div>
  }

  return <TestCasePanelEntry test_case={selectedTestCase} />
}

const EditTestCaseForm = ({
  testCase,
  schema,
  func,
  getTestParams,
  children,
  duplicate,
}: {
  func: Func
  testCase?: TestCase
  schema: any
  getTestParams: (testCase: TestCase) => void
  children: React.ReactNode
  duplicate?: boolean
}) => {
  const { root_path } = useContext(ASTContext)

  // TODO, actually fix this for named args
  const formData = useMemo(() => {
    if (testCase === undefined) {
      return null
    }
    try {
      return JSON.parse(testCase.content)
    } catch (e) {
      console.warn('Error parsing data, will default to string\n' + JSON.stringify(testCase), e)
      return testCase.content
    }
  }, [testCase?.content])

  const [showForm, setShowForm] = useState(false)
  const [testName, setTestName] = useState<string | undefined>(
    duplicate ? `${testCase?.name.value}-copy` : testCase?.name.value,
  )

  return (
    <Dialog open={showForm} onOpenChange={setShowForm}>
      <DialogTrigger asChild={true}>{children}</DialogTrigger>
      <DialogContent className='overflow-y-scroll max-h-screen bg-vscode-editorWidget-background border-vscode-textSeparator-foreground overflow-x-clip'>
        <DialogHeader className='flex flex-row gap-x-4 items-center'>
          <DialogTitle className='text-xs font-semibold'>{duplicate ? 'Duplicate test' : 'Edit test'}</DialogTitle>

          <div className='flex flex-row gap-x-2 items-center pb-1'>
            {testCase === undefined || duplicate ? (
              <VSCodeTextField
                className='w-32'
                value={testName === undefined ? '' : testName}
                placeholder='Enter test name'
                onInput={(e) => {
                  // weird things happen if we dont do the replacement here -- on prompt fiddle
                  // it seems new updates to tests dont get propagated if spaces are set.
                  setTestName(
                    (e as React.FormEvent<HTMLInputElement>).currentTarget.value.replace(
                      /[&\/\\#, +()$~%.'":*?<>{}]/g,
                      '_',
                    ),
                  )
                }}
              />
            ) : (
              // for now we dont support renaming existing test
              <div>{testName}</div>
            )}
          </div>
        </DialogHeader>
        <Form
          schema={schema}
          formData={formData}
          validator={validator}
          uiSchema={uiSchema}
          templates={TEMPLATES}
          onSubmit={(data) => {
            vscode.postMessage({
              command: 'saveTest',
              data: {
                root_path,
                funcName: func.name.value,
                testCaseName: testName, // a stringspan or string
                params: getTestParams({
                  ...(testCase ?? {
                    name: {
                      value: 'new',
                      source_file: '',
                      start: 0,
                      end: 0,
                    },
                    content: 'null',
                  }),
                  content: JSON.stringify(data.formData, null, 2),
                }),
              },
            })
            setShowForm(false)
            if (testCase === undefined) {
              // reset the name back to undefined since this component is used to add new tests
              setTestName(undefined)
            }
          }}
        />
      </DialogContent>
    </Dialog>
  )
}

const TestCaseCard: React.FC<{ test_case: WasmTestCase; isRendered: boolean }> = ({ test_case, isRendered }) => {
  return (
    <div className='flex flex-col gap-2 max-w-full text-xs text-left truncate text-vscode-descriptionForeground'>
      {test_case.error && (
        <pre className='break-words whitespace-pre-wrap w-full border-vscode-textSeparator-foreground rounded-md border p-0.5'>
          {test_case.error}
        </pre>
      )}
      <div className='whitespace-pre-wrap break-all'>
        <div className='flex flex-col'>
          {test_case.inputs.map((input) => (
            <div key={input.name}>
              <b>{input.name}:</b>
              {input.value !== undefined && (
                <JsonView
                  enableClipboard
                  className='bg-[#1E1E1E] px-1 py-1 rounded-sm'
                  theme='a11y'
                  collapseStringsAfterLength={600}
                  matchesURL
                  editable={true}
                  src={JSON.parse(input.value)}
                />
              )}
              {input.error && (
                <pre className='break-words whitespace-pre-wrap w-full border-vscode-textSeparator-foreground rounded-md border p-0.5'>
                  {input.error}
                </pre>
              )}
            </div>
          ))}
        </div>
        {/* {test_case.content.substring(0, 120)}
        {test_case.content.length > 120 && '...'} */}
      </div>
    </div>
  )
}

export default TestCasePanel
