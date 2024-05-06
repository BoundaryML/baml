/// Content once a function has been selected.

import { UiSchema } from '@rjsf/utils'

import { uniqueNamesGenerator, Config, adjectives, colors, animals } from 'unique-names-generator'
import { JSONSchemaFaker as jsf } from 'json-schema-faker'
import { Button } from '../components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '../components/ui/dialog'
import { vscode } from '../utils/vscode'
import { ParserDatabase, StringSpan, TestRequest } from '@baml/common'
import Form, { getDefaultRegistry } from '@rjsf/core'
import validator from '@rjsf/validator-ajv8'
import { VSCodeButton, VSCodeProgressRing, VSCodeTextArea, VSCodeTextField } from '@vscode/webview-ui-toolkit/react'
import { Copy, Edit2, FileJson2, Save, Play, PlusIcon, Trash2, Pin } from 'lucide-react'
import React, { ChangeEvent, FocusEvent, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import { ASTContext } from './ASTProvider'
import TypeComponent from './TypeComponent'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../components/ui/tooltip'
import { TEMPLATES } from './TestCaseEditor/JsonEditorTemplates'
import JsonView from 'react18-json-view'
import { Badge } from '../components/ui/badge'
import { useAtom, useAtomValue } from 'jotai'
import { selectedFunctionAtom, selectedTestCaseAtom } from '@/baml_wasm_web/EventListener'
import type { WasmTestCase } from '@gloo-ai/baml-schema-wasm-web/baml_schema_build'
import { useRunHooks } from '@/baml_wasm_web/test_uis/testHooks'

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
  const [selectedTestCase, setSelected] = useAtom(selectedTestCaseAtom)
  const isRendered = useMemo(() => selectedTestCase?.name === test_case.name, [selectedTestCase, test_case])
  const { run, isRunning } = useRunHooks();

  return (
    <div key={test_case.name} className="flex flex-col py-1 pr-2 w-full overflow-x-clip group">
      <div className="flex flex-row justify-between items-center">
        <div className="flex flex-row gap-x-1 justify-center items-center">
          <Button
            variant={'ghost'}
            size={'icon'}
            className="p-1 rounded-md w-fit h-fit bg-vscode-button-background text-vscode-button-foreground hover:bg-vscode-button-hoverBackground"
            disabled={isRunning}
            onClick={() => {
              run(test_case.name)
            }}
          >
            <Play size={10} />
          </Button>
          {/* IDK why it doesnt truncate. Probably cause of the allotment */}
          <div className="flex flex-nowrap w-full">
            <span className="h-[24px] max-w-[120px] text-center align-middle overflow-hidden flex-1 truncate">
              {test_case.name}
            </span>
            {isRendered && (
              <Badge
                className="ml-2 bg-vscode-editorSuggestWidget-selectedBackground text-vscode-editorSuggestWidget-foreground hover:bg-vscode-editorSuggestWidget-selectedBackground"
                variant="default"
              >
                <div className="flex flex-row gap-x-1 items-center">
                  <Pin size={12} /> Rendered
                </div>
              </Badge>
            )}
            <div className="hidden gap-x-1 group-hover:flex">
              {!isRendered && (
                <Button
                  variant={'ghost'}
                  size="icon"
                  className="p-1 w-fit h-fit hover:bg-vscode-button-secondaryHoverBackground"
                  onClick={() => {
                    setSelected(test_case.name)
                  }}
                >
                  <Pin size={12} />
                </Button>
              )}
              {/* <EditTestCaseForm
                testCase={test_case}
                schema={input_json_schema}
                func={func}
                getTestParams={(t) => getTestParams(func, t)}
              >
                <Button
                  variant={'ghost'}
                  size="icon"
                  className="p-1 w-fit h-fit hover:bg-vscode-button-secondaryHoverBackground"
                >
                  <Edit2 className="w-3 h-3 text-vscode-descriptionForeground" />
                </Button>
              </EditTestCaseForm> */}
              <Tooltip delayDuration={100}>
                <TooltipTrigger asChild>
                  <Button
                    variant={'ghost'}
                    size={'icon'}
                    className="p-1 w-fit h-fit text-vscode-descriptionForeground hover:bg-vscode-button-secondaryHoverBackground"
                    onClick={() => {
                      vscode.postMessage({ command: 'jumpToFile', data: test_case.name })
                    }}
                  >
                    <FileJson2 size={14} />
                  </Button>
                </TooltipTrigger>
                <TooltipContent className="flex flex-col gap-y-1">Open test file</TooltipContent>
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
          className="p-1 w-fit h-fit text-vscode-input-foreground"
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
      <TestCaseCard test_case={test_case} isRendered={isRendered} />
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
    let parsed = JSON.parse(testCase.content)
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
  const selectedFunction = useAtomValue(selectedFunctionAtom)
  const testCases = useMemo(() => selectedFunction?.test_cases ?? [], [selectedFunction])

  const [filter, setFilter] = useState<string>('')
  const { root_path, test_results } = useContext(ASTContext)

  return (
    <div className="flex flex-col w-full h-full tour-test-panel">
      <div className="flex flex-row gap-x-1">
        <VSCodeTextField
          placeholder="Search test cases"
          className="w-32 shrink"
          value={filter}
          onInput={(e) => {
            setFilter((e as React.FormEvent<HTMLInputElement>).currentTarget.value)
          }}
        />
        {test_results?.run_status === 'RUNNING' ? (
          <VSCodeButton
            className="bg-vscode-statusBarItem-errorBackground"
            onClick={() => vscode.postMessage({ command: 'cancelTestRun' })}
          >
            Cancel
          </VSCodeButton>
        ) : (
          <>
            <Button
              className="px-1 py-1 h-full text-xs whitespace-nowrap bg-red-500 rounded-sm bg-vscode-button-background text-vscode-button-foreground hover:bg-vscode-button-hoverBackground"
              // disabled={test_cases.length === 0}
              onClick={() => {
                // const runTestRequest: TestRequest = {
                //   functions: [
                //     {
                //       name: func.name.value,
                //       run_all_available_tests: filter === '' ? true : false,
                //       tests: test_cases.map((test_case) => ({
                //         name: test_case.name.value,
                //         impls: func.impls.map((i) => i.name.value),
                //       })),
                //     },
                //   ],
                // }
                // vscode.postMessage({
                //   command: 'runTest',
                //   data: {
                //     root_path,
                //     tests: runTestRequest,
                //   },
                // })
              }}
            >
              <>Run {filter ? testCases.length : 'all'}</>
            </Button>
          </>
        )}
      </div>
      <div className="flex flex-col gap-y-1 py-2 divide-y divide-vscode-textSeparator-foreground">
        {/* <pre>{JSON.stringify(input_json_schema, null, 2)}</pre> */}
        {/* <EditTestCaseForm
          key={'new'}
          testCase={undefined}
          schema={input_json_schema}
          func={func}
          getTestParams={(t) => getTestParams(func, t)}
        >
          <Button className="flex flex-row gap-x-2 text-sm bg-vscode-dropdown-background text-vscode-dropdown-foreground hover:opacity-90 hover:bg-vscode-dropdown-background">
            <PlusIcon size={16} />
            <div>Add test case</div>
          </Button>
        </EditTestCaseForm> */}

        {testCases.map((t) => (
          <TestCasePanelEntry test_case={t} />
        ))}
      </div>
    </div>
  )
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
      <DialogContent className="overflow-y-scroll max-h-screen bg-vscode-editorWidget-background border-vscode-textSeparator-foreground overflow-x-clip">
        <DialogHeader className="flex flex-row gap-x-4 items-center">
          <DialogTitle className="text-xs font-semibold">{duplicate ? 'Duplicate test' : 'Edit test'}</DialogTitle>

          <div className="flex flex-row gap-x-2 items-center pb-1">
            {testCase === undefined || duplicate ? (
              <VSCodeTextField
                className="w-32"
                value={testName === undefined ? '' : testName}
                placeholder="Enter test name"
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
    <div className="flex flex-col gap-2 max-w-full text-xs text-left truncate text-vscode-descriptionForeground">
      <div className="whitespace-pre-wrap break-all">
        <div className="flex flex-col">
          {test_case.inputs.map((input) => (
            <div key={input.name}>
              <b>{input.name}:</b> {input.value}
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
