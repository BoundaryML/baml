/// Content once a function has been selected.

import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { vscode } from '@/utils/vscode'
import { ParserDatabase, StringSpan, TestRequest } from '@baml/common'
import Form, { getDefaultRegistry } from '@rjsf/core'
import {
  ArrayFieldTemplateItemType,
  ArrayFieldTitleProps,
  BaseInputTemplateProps,
  FieldTemplateProps,
  IconButtonProps,
  ObjectFieldTemplateProps,
  RJSFSchema,
  UiSchema,
  ariaDescribedByIds,
  examplesId,
  getInputProps,
  titleId,
} from '@rjsf/utils'
import validator from '@rjsf/validator-ajv8'
import { VSCodeButton, VSCodeProgressRing, VSCodeTextArea, VSCodeTextField } from '@vscode/webview-ui-toolkit/react'
import { Copy, Edit2, FileJson2, Play, PlusIcon, X } from 'lucide-react'
import { ChangeEvent, FocusEvent, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import { ASTContext } from './ASTProvider'
import TypeComponent from './TypeComponent'
import { useSelections } from './hooks'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'

const testSchema: RJSFSchema = {
  title: 'Test form',
  type: 'object',
  properties: {
    name: {
      type: 'string',
    },
    age: {
      type: 'number',
    },
  },
}
const {
  templates: { BaseInputTemplate, FieldTemplate, ButtonTemplates },
} = getDefaultRegistry()

function MyBaseInputTemplate(props: BaseInputTemplateProps) {
  const {
    id,
    name, // remove this from ...rest
    value,
    readonly,
    disabled,
    autofocus,
    onBlur,
    onFocus,
    onChange,
    onChangeOverride,
    options,
    schema,
    uiSchema,
    formContext,
    registry,
    rawErrors,
    type,
    hideLabel, // remove this from ...rest
    hideError, // remove this from ...rest
    ...rest
  } = props

  // Note: since React 15.2.0 we can't forward unknown element attributes, so we
  // exclude the "options" and "schema" ones here.
  if (!id) {
    console.log('No id for', props)
    throw new Error(`no id for props ${JSON.stringify(props)}`)
  }
  const inputProps = {
    ...rest,
    ...getInputProps(schema, type, options),
  }

  let inputValue
  if (inputProps.type === 'number' || inputProps.type === 'integer') {
    inputValue = value || value === 0 ? value : ''
  } else {
    inputValue = value == null ? '' : value
  }

  const _onChange = useCallback(
    ({ target: { value } }: ChangeEvent<HTMLInputElement>) => onChange(value === '' ? options.emptyValue : value),
    [onChange, options],
  )
  const _onBlur = useCallback(({ target: { value } }: FocusEvent<HTMLInputElement>) => onBlur(id, value), [onBlur, id])
  const _onFocus = useCallback(
    ({ target: { value } }: FocusEvent<HTMLInputElement>) => onFocus(id, value),
    [onFocus, id],
  )

  const length = Object.keys(registry.rootSchema?.definitions ?? {}).length

  const isSingleStringField = length === 0 && schema.type === 'string'

  const input =
    inputProps.type === 'number' || inputProps.type === 'integer' ? (
      <input
        id={id}
        name={id}
        className="max-w-[100px] rounded-sm bg-vscode-input-background text-vscode-input-foreground"
        readOnly={readonly}
        disabled={disabled}
        autoFocus={autofocus}
        value={inputValue}
        {...inputProps}
        list={schema.examples ? examplesId(id) : undefined}
        onChange={onChangeOverride || _onChange}
        onBlur={_onBlur}
        onFocus={_onFocus}
        aria-describedby={ariaDescribedByIds(id, !!schema.examples)}
      />
    ) : (
      <textarea
        id={id}
        name={id}
        rows={isSingleStringField ? 25 : 5}
        className="w-[90%] px-1 rounded-sm bg-vscode-input-background text-vscode-input-foreground"
        readOnly={readonly}
        disabled={disabled}
        autoFocus={autofocus}
        value={inputValue}
        {...inputProps}
        // list={schema.examples ? examplesId(id) : undefined}
        onChange={(onChangeOverride as any) || _onChange}
        onBlur={_onBlur as any}
        onFocus={_onFocus as any}
        aria-describedby={ariaDescribedByIds(id, !!schema.examples)}
      />
    )

  return (
    <div className="flex flex-col w-full gap-y-1">
      {input}
      {Array.isArray(schema.examples) && (
        <datalist key={`datalist_${id}`} id={examplesId(id)}>
          {(schema.examples as string[])
            .concat(schema.default && !schema.examples.includes(schema.default) ? ([schema.default] as string[]) : [])
            .map((example: any) => {
              return <option key={example} value={example} />
            })}
        </datalist>
      )}
    </div>
  )
}

// function MyFieldTemplate(props: FieldTemplateProps) {
//   return <FieldTemplate {...props} classNames="  gap-x-2" />
// }

function MyFieldTemplate(props: FieldTemplateProps) {
  const { id, classNames, style, label, displayLabel, help, required, hidden, description, errors, children } = props

  if (hidden) {
    return <div className="hidden">{children}</div>
  }

  return (
    <div className={classNames + ' ml-2 w-full'} style={style}>
      <>
        {props.schema.type === 'boolean' ? null : (
          <label htmlFor={id} className="flex flex-row items-center gap-x-3">
            <div className={props.schema.type === 'object' ? ' font-bold text-sm' : ' text-xs'}>
              {label.split('-').at(-1)}
            </div>
            <div className={'text-vscode-textSeparator-foreground'}>
              {props.schema.type}
              {required ? '*' : null}
            </div>
          </label>
        )}
      </>

      {description}
      <div className="flex flex-row items-center w-full">{children}</div>
      {errors}
      {help}
    </div>
  )
}

function MyObjectTemplate(props: ObjectFieldTemplateProps) {
  return (
    <div className="w-full">
      {/* <div className="py-2">{props.title}</div> */}
      {props.description}
      <div className="flex flex-col w-full py-1 gap-y-2">
        {props.properties.map((element) => (
          <div className="w-full property-wrapper text-vscode-input-foreground">{element.content}</div>
        ))}
      </div>
    </div>
  )
}

function AddButton(props: IconButtonProps) {
  const { icon, iconType, ...btnProps } = props
  return (
    <Button
      variant="ghost"
      size="icon"
      {...btnProps}
      className="flex flex-row items-center p-1 text-xs w-fit h-fit gap-x-2 hover:bg-vscode-descriptionForeground"
    >
      <PlusIcon size={16} /> <div>Add item</div>
    </Button>
  )
}

function RemoveButton(props: IconButtonProps) {
  const { icon, iconType, ...btnProps } = props
  return (
    <div className="flex w-fit h-fit">
      <Button
        {...btnProps}
        size={'icon'}
        className="!flex flex-col !px-0 !py-0 bg-red-700 h-fit !max-w-[48px] ml-auto"
        style={{
          flex: 'none',
        }}
      >
        <X size={12} />
      </Button>
    </div>
  )
}

function SubmitButton(props: IconButtonProps) {
  const { icon, iconType, ...btnProps } = props
  return (
    <div className="flex items-end justify-end w-full ml-auto h-fit">
      <Button
        {...btnProps}
        className="px-3 py-2 rounded-none bg-vscode-button-background text-vscode-button-foreground w-fit h-fit hover:bg-vscode-button-background hover:opacity-75"
        style={{
          flex: 'none',
        }}
      >
        Submit
      </Button>
    </div>
  )
}

function ArrayFieldItemTemplate(props: ArrayFieldTemplateItemType) {
  const { children, className } = props
  return (
    <div className="relative ">
      <div className={`${className} ml-0 py-1 text-xs text-vscode-descriptionForeground`}>{children}</div>
      {props.hasRemove && (
        <div className="absolute top-0 flex ml-auto right-4 w-fit h-fit">
          <Button
            onClick={props.onDropIndexClick(props.index)}
            disabled={props.disabled || props.readonly}
            size={'icon'}
            className="p-1 bg-transparent w-fit h-fit hover:bg-red-700"
            style={{
              flex: 'none',
            }}
          >
            <X size={12} />
          </Button>
        </div>
      )}
    </div>
  )
}

function ArrayFieldTitleTemplate(props: ArrayFieldTitleProps) {
  const { title, idSchema } = props
  const id = titleId(idSchema)
  return null
  // return (
  //   <div id={id} className="text-xs">
  //     {title}
  //   </div>
  // )
}

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

type Func = ParserDatabase['functions'][0]

const TestCasePanel: React.FC<{ func: Func }> = ({ func }) => {
  const { impl, input_json_schema } = useSelections()

  const [filter, setFilter] = useState<string>('')
  const test_cases = useMemo(() => {
    if (!filter) return func.test_cases
    return func.test_cases.filter(
      (test_case) => test_case.name.value.includes(filter) || test_case.content.includes(filter),
    )
  }, [filter, func.test_cases])

  const getTestParams = (testCase: Func['test_cases'][0]) => {
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
        value: func.input.values.map(({ name }) => ({
          name: name.value,
          value: contentMap.get(name.value) ?? null,
        })),
      }
    }
  }
  const { root_path, test_results } = useContext(ASTContext)

  return (
    <>
      <div className="flex flex-row justify-between gap-x-1">
        <VSCodeTextField
          placeholder="Search test cases"
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
            <VSCodeButton
              disabled={test_cases.length === 0}
              onClick={() => {
                const runTestRequest: TestRequest = {
                  functions: [
                    {
                      name: func.name.value,
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
              <>Run {filter ? test_cases.length : 'all'} tests</>
            </VSCodeButton>
          </>
        )}
      </div>
      <div className="flex flex-col py-2 divide-y gap-y-1 divide-vscode-textSeparator-foreground">
        {/* <pre>{JSON.stringify(input_json_schema, null, 2)}</pre> */}
        <EditTestCaseForm testCase={undefined} schema={input_json_schema} func={func} getTestParams={getTestParams}>
          <Button className="flex flex-row text-sm gap-x-2 bg-vscode-dropdown-background text-vscode-dropdown-foreground hover:opacity-90 hover:bg-vscode-dropdown-background">
            <PlusIcon size={16} />
            <div>Add test case</div>
          </Button>
        </EditTestCaseForm>
        {test_cases.map((test_case) => (
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
                    vscode.postMessage({
                      command: 'runTest',
                      data: {
                        root_path,
                        tests: runTestRequest,
                      },
                    })
                  }}
                >
                  <Play size={10} />
                </Button>
                <div className="h-[24px] text-center align-middle max-w-[110px] truncate">{test_case.name.value}</div>
                <div className="flex-row hidden gap-x-1 group-hover:flex ">
                  <EditTestCaseForm
                    testCase={test_case}
                    schema={input_json_schema}
                    func={func}
                    getTestParams={getTestParams}
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

                  <EditTestCaseForm
                    testCase={test_case}
                    schema={input_json_schema}
                    func={func}
                    getTestParams={getTestParams}
                    duplicate
                  >
                    <Tooltip delayDuration={100}>
                      <TooltipTrigger asChild>
                        <Button
                          variant={'ghost'}
                          size="icon"
                          className="p-1 w-fit h-fit hover:bg-vscode-button-secondaryHoverBackground"
                        >
                          <Copy size={12} />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent className="flex flex-col gap-y-1">Duplicate</TooltipContent>
                    </Tooltip>
                  </EditTestCaseForm>
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
                <X size={10} />
              </Button>
            </div>
            <EditTestCaseForm testCase={test_case} schema={input_json_schema} func={func} getTestParams={getTestParams}>
              <Button
                variant={'ghost'}
                className="items-start justify-start w-full px-1 py-1 text-left hover:bg-vscode-button-secondaryHoverBackground h-fit"
              >
                <TestCaseCard content={test_case.content} testCaseName={test_case.name.value} />
              </Button>
            </EditTestCaseForm>
          </div>
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
    if (testCase === undefined) return {}
    try {
      return JSON.parse(testCase?.content)
    } catch (e) {
      console.log('Error parsing data\n' + JSON.stringify(testCase), e)
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
          <DialogTitle className="text-xs font-semibold">Edit Test </DialogTitle>

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
          templates={{
            BaseInputTemplate: MyBaseInputTemplate,
            FieldTemplate: MyFieldTemplate,
            ObjectFieldTemplate: MyObjectTemplate,
            ButtonTemplates: {
              AddButton,
              // RemoveButton,
              SubmitButton,
            },
            ArrayFieldTitleTemplate: ArrayFieldTitleTemplate,
            ArrayFieldItemTemplate: ArrayFieldItemTemplate,
          }}
          onSubmit={(data) => {
            const testCaseToSave = testCase ?? {
              name: {
                value: 'new',
                source_file: '',
                start: 0,
                end: 0,
              },
              content: 'null',
            }

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
          }}
        />
      </DialogContent>
    </Dialog>
  )
}

const TestCaseCard: React.FC<{ testCaseName: string; content: string }> = ({ testCaseName, content }) => {
  return (
    <div className="flex flex-col gap-2 text-xs text-left text-vscode-descriptionForeground">
      <div className="text-balance">
        {content.substring(0, 120)}
        {content.length > 120 && '...'}
      </div>
    </div>
  )
}

export default TestCasePanel
