/// Content once a function has been selected.

import { UiSchema, } from '@rjsf/utils'

import { uniqueNamesGenerator, Config, adjectives, colors, animals } from 'unique-names-generator'
import { JSONSchemaFaker as jsf } from 'json-schema-faker'
import { Button } from '../components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '../components/ui/dialog'
import { vscode } from '../utils/vscode'
import { ParserDatabase, StringSpan, TestRequest } from '@baml/common'
import Form, { getDefaultRegistry } from '@rjsf/core'
import validator from '@rjsf/validator-ajv8'
import { VSCodeButton, VSCodeProgressRing, VSCodeTextArea, VSCodeTextField } from '@vscode/webview-ui-toolkit/react'
import { Copy, Edit2, FileJson2, Save, Play, PlusIcon, Trash2 } from 'lucide-react'
import React, { ChangeEvent, FocusEvent, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import { ASTContext } from './ASTProvider'
import TypeComponent from './TypeComponent'
import { useSelections } from './hooks'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../components/ui/tooltip'
import { TEMPLATES } from "./TestCaseEditor/JsonEditorTemplates"

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
type TestCase = Func['test_cases'][number] & {
  saved: boolean
}


const TestCasePanelEntry: React.FC<{ func: Func; test_case: TestCase }> = ({ func, test_case }) => {
  const { impl, input_json_schema } = useSelections()
  if (input_json_schema) {
    input_json_schema.definitions = Object.fromEntries(
      Object.entries(input_json_schema.definitions as object).map(([k, v]) => [k, {...v, title: k}])
    )
  }
  const { root_path, test_results } = useContext(ASTContext)
  return (
    <div key={test_case.name.value} className="py-1 group">
      <div className="flex flex-row items-center justify-between">
        <div className="flex flex-row items-center justify-center gap-x-1">
          <Button
            variant={'ghost'}
            size={'icon'}
            className="p-1 rounded-md w-fit h-fit bg-vscode-button-background text-vscode-button-foreground hover:bg-vscode-button-hoverBackground"
            disabled={impl === undefined || test_results?.run_status === 'RUNNING'}
            onClick={() => {
              const runTestRequest: TestRequest = {
                functions: [
                  {
                    name: func.name.value,
                    tests: [
                      {
                        name: test_case.name.value,
                        impls: impl ? [impl.name.value] : [],
                      },
                    ],
                  },
                ],
              }
              if (test_case.saved) {
                vscode.postMessage({
                  command: 'runTest',
                  data: {
                    root_path,
                    tests: runTestRequest,
                  },
                })
              } else {
                vscode.postMessage({
                  command: 'commandSequence',
                  data: [{
                    command: 'saveTest',
                    data: {
                      root_path,
                      funcName: func.name.value,
                      testCaseName: test_case.name, // a stringspan or string
                      params: getTestParams(func, test_case),
                    },
                  }, {
                    command: 'runTest',
                    data: {
                      root_path,
                      tests: runTestRequest,
                    },
                  }],
                })
              }
            }}
          >
            {test_case.saved ? (
              <Play size={10}/>
            ) : (
              <div className="flex flex-row">
                <Save size={10} className="text-vscode-gitDecoration-modifiedResourceForeground"/>
                <Play size={10} className="text-vscode-gitDecoration-modifiedResourceForeground"/>
              </div>
            )}
          </Button>
          {/* IDK why it doesnt truncate. Probably cause of the allotment */}
          <div className="flex w-full flex-nowrap">
            <span className={
              test_case.saved 
              ? "h-[24px] max-w-[120px] text-center align-middle overflow-hidden flex-1 truncate"
              : "h-[24px] max-w-[120px] text-center align-middle overflow-hidden flex-1 truncate text-vscode-gitDecoration-modifiedResourceForeground"
            }>
              {test_case.name.value}
            </span>
            <div className="hidden gap-x-1 group-hover:flex">
              <EditTestCaseForm
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
              </EditTestCaseForm>
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
              <Tooltip delayDuration={100}>
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
              </Tooltip>
            </div>
          </div>
        </div>
        <Button
          variant={'ghost'}
          size={'icon'}
          className="p-1 w-fit h-fit text-vscode-input-foreground"
          onClick={() => {
            vscode.postMessage({
              command: 'removeTest',
              data: {
                root_path,
                funcName: func.name.value,
                testCaseName: test_case.name,
              },
            })
          }}
        >
          <Trash2 size={10} />
        </Button>
      </div>
      <EditTestCaseForm
        testCase={test_case}
        schema={input_json_schema}
        func={func}
        getTestParams={(t) => getTestParams(func, t)}
      >
        <Button
          variant={'ghost'}
          className="items-start justify-start w-full px-1 py-1 text-left hover:bg-vscode-button-secondaryHoverBackground h-fit"
        >
          <TestCaseCard test_case={test_case} />
        </Button>
      </EditTestCaseForm>
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
      value: func.input.values.map(({ name } : {name: StringSpan }) => ({
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
      }) as string,
    },
    content: JSON.stringify(jsf.generate(input_json_schema)),
    saved: false,
  }
};

const TestCasePanel: React.FC<{ func: Func }> = ({ func }) => {
  const { impl, input_json_schema } = useSelections()

  const [filter, setFilter] = useState<string>('')
  // This should be re-generated when this test case is saved
  const test_cases = useMemo(() => {
    console.log("input json schema", JSON.stringify(input_json_schema, null, 2))
    let test_cases = func.test_cases.map((t) => ({ ...t, saved: true }))
    if (filter) {
      test_cases = test_cases.filter((test_case) => test_case.name.value.includes(filter) || test_case.content.includes(filter))
    }
    if (test_cases.length === 0) {
      return [autoGenTestCase(func, input_json_schema)]
    }
    return test_cases
  }, [filter, func])

  const { root_path, test_results } = useContext(ASTContext)

  return (
    <>
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
              className="px-1 py-1 text-sm bg-red-500 rounded-sm h-fit whitespace-nowrap bg-vscode-button-background text-vscode-button-foreground hover:bg-vscode-button-hoverBackground"
              // disabled={test_cases.length === 0}
              onClick={() => {
                const runTestRequest: TestRequest = {
                  functions: [
                    {
                      name: func.name.value,
                      run_all_available_tests: filter === '' ? true : false,
                      tests: test_cases.map((test_case) => ({
                        name: test_case.name.value,
                        impls: func.impls.map((i) => i.name.value),
                      })),
                    },
                  ],
                }
                vscode.postMessage({
                  command: 'runTest',
                  data: {
                    root_path,
                    tests: runTestRequest,
                  },
                })
              }}
            >
              <>Run {filter ? test_cases.length : 'all'}</>
            </Button>
          </>
        )}
      </div>
      <div className="flex flex-col py-2 divide-y gap-y-1 divide-vscode-textSeparator-foreground">
        {/* <pre>{JSON.stringify(input_json_schema, null, 2)}</pre> */}
        <EditTestCaseForm
          testCase={undefined}
          schema={input_json_schema}
          func={func}
          getTestParams={(t) => getTestParams(func, t)}
        >
          <Button className="flex flex-row text-sm gap-x-2 bg-vscode-dropdown-background text-vscode-dropdown-foreground hover:opacity-90 hover:bg-vscode-dropdown-background">
            <PlusIcon size={16} />
            <div>Add test case</div>
          </Button>
        </EditTestCaseForm>

        {
          test_cases.some((t) => !t.saved) && (
            <div className="rounded-md w-fit font-sans">We've automatically created a test case for you! Click the button to save and run.</div>
          )
        }
        {test_cases.map((t) => (
          <TestCasePanelEntry func={func} test_case={t} />
        ))}
      </div>
    </>
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
  testCase?: Func['test_cases'][0]
  schema: any
  getTestParams: (testCase: Func['test_cases'][0]) => void
  children: React.ReactNode
  duplicate?: boolean
}) => {
  const { root_path } = useContext(ASTContext)

  // TODO, actually fix this for named args
  const formData = useMemo(() => {
    if (testCase === undefined) {
      jsf.option({
        alwaysFakeOptionals: true,
        minItems: 2,
        maxItems: 2,
      })
      const fakeData = jsf.generate(schema)
      console.log('making fake data')
      return fakeData
    }
    try {
      return JSON.parse(testCase?.content)
    } catch (e) {
      console.warn('Error parsing data, will default to string\n' + JSON.stringify(testCase), e)
      return testCase?.content ?? 'null'
    }
  }, [testCase?.content])

  const [showForm, setShowForm] = useState(false)
  const [testName, setTestName] = useState(duplicate ? undefined : testCase?.name.value)

  return (
    <Dialog open={showForm} onOpenChange={setShowForm}>
      <DialogTrigger asChild={true}>{children}</DialogTrigger>
      <DialogContent className="max-h-screen overflow-y-scroll bg-vscode-editorWidget-background border-vscode-textSeparator-foreground overflow-x-clip">
        <DialogHeader className="flex flex-row items-center gap-x-4">
          <DialogTitle className="text-xs font-semibold">{duplicate ? 'Duplicate test' : 'Edit test'}</DialogTitle>

          <div className="flex flex-row items-center pb-1 gap-x-2">
            {testCase === undefined || duplicate ? (
              <VSCodeTextField
                className="w-32"
                value={testName === undefined ? '' : testName}
                placeholder="Enter test name"
                onInput={(e) => {
                  setTestName((e as React.FormEvent<HTMLInputElement>).currentTarget.value)
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
            setTestName(undefined)
          }}
        />
      </DialogContent>
    </Dialog>
  )
}

const TestCaseCard: React.FC<{ test_case: TestCase }> = ({ test_case }) => {
  return (
    <div className="flex flex-col max-w-full gap-2 text-xs text-left text-vscode-descriptionForeground">
      <div className={ test_case.saved ? "break-all" : "break-all text-vscode-gitDecoration-modifiedResourceForeground"}>
        {test_case.content.substring(0, 120)}
        {test_case.content.length > 120 && '...'}
      </div>
    </div>
  )
}

export default TestCasePanel
