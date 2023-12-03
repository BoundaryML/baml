/// Content once a function has been selected.

import { ParserDatabase, StringSpan, TestRequest } from '@baml/common'
import { useSelections } from './hooks'
import TypeComponent from './TypeComponent'
import { VSCodeButton, VSCodeTextArea, VSCodeTextField } from '@vscode/webview-ui-toolkit/react'
import { ChangeEvent, useContext, useEffect, useMemo, useState, FocusEvent, useCallback } from 'react'
import { vscode } from '@/utils/vscode'
import { ASTContext } from './ASTProvider'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { Separator } from '@/components/ui/separator'
import { Button } from '@/components/ui/button'
import { Edit, Edit2, Play, PlusIcon, X } from 'lucide-react'
import { TestRunRequest } from 'vscode'
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
import { Dialog, DialogContent, DialogTrigger } from '@/components/ui/dialog'
import Form from '@rjsf/core'
import { getDefaultRegistry } from '@rjsf/core'

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
  widgets,
} = getDefaultRegistry()

function MyBaseInputTemplate(props: BaseInputTemplateProps) {
  const customProps = {}
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

  const input =
    inputProps.type === 'number' || inputProps.type === 'integer' ? (
      <input
        id={id}
        name={id}
        className="form-control"
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
        rows={3}
        className="min-w-[400px] form-control px-1"
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
    <>
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
    </>
  )
}

function MyFieldTemplate(props: FieldTemplateProps) {
  return <FieldTemplate {...props} classNames="  gap-x-2" />
}

function MyObjectTemplate(props: ObjectFieldTemplateProps) {
  return (
    <div>
      <div className="py-2">{props.title}</div>
      {props.description}
      <div className="flex flex-col items-start justify-start gap-y-1">
        {props.properties.map((element) => (
          <div className="ml-4 property-wrapper text-vscode-input-foreground">{element.content}</div>
        ))}
      </div>
    </div>
  )
}

function AddButton(props: IconButtonProps) {
  const { icon, iconType, ...btnProps } = props
  return (
    <Button variant="ghost" size="icon" {...btnProps} className="p-1 w-fit h-fit">
      <PlusIcon size={16} />
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

function ArrayFieldItemTemplate(props: ArrayFieldTemplateItemType) {
  const { children, className } = props
  return (
    <div>
      <div className={`${className} ml-2 text-xs text-vscode-descriptionForeground`}>{children}</div>
      {props.hasRemove && (
        <div className="flex ml-auto w-fit h-fit">
          <Button
            onClick={props.onDropIndexClick(props.index)}
            disabled={props.disabled || props.readonly}
            size={'icon'}
            className="p-1 bg-red-700 w-fit h-fit"
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
  return (
    <div id={id} className="text-xs">
      {title}
    </div>
  )
}

const uiSchema: UiSchema = {
  'ui:submitButtonOptions': {
    submitText: 'Save',
    props: {
      className: 'bg-vscode-button-background px-2',
    },
  },
  // 'ui:widget': 'textarea',

  'ui:autocomplete': 'on',
  'ui:options': {
    removable: true,
  },
}

type Func = ParserDatabase['functions'][0]

const TestCasePanel: React.FC<{ func: Func }> = ({ func }) => {
  const test_cases = func?.test_cases.map((cases) => cases) ?? []
  const { impl, input_json_schema } = useSelections()

  const getTestParams = (testCase: Func['test_cases'][0]): TestRequest['functions'][0]['tests'][0]['params'] => {
    if (func.input.arg_type === 'positional') {
      return {
        type: 'positional',
        value: testCase.content,
      }
    } else {
      let parsed = JSON.parse(testCase.content)
      let contentMap = new Map<string, string>()
      if (typeof parsed === 'object') {
        contentMap = new Map(Object.entries(parsed).map(([k, v]) => [k, JSON.stringify(v, null, 2)]))
      }
      return {
        type: 'named',
        value: func.input.values.map(({ name }) => ({
          name: name.value,
          value: contentMap.get(name.value) ?? '',
        })),
      }
    }
  }
  // const { test_case } = useSelections()
  return (
    <>
      <div>Test Cases</div>
      <div className="flex flex-row justify-between">
        <VSCodeTextField placeholder="Search test cases" />
        <VSCodeButton
          onClick={() => {
            const runTestRequest: TestRequest = {
              functions: [
                {
                  name: func.name.value,
                  tests: test_cases.map((test_case) => ({
                    name: test_case.name.value,
                    params: getTestParams(test_case),
                    impls: impl ? [impl.name.value] : [],
                  })),
                },
              ],
            }
            vscode.postMessage({
              command: 'runTest',
              data: runTestRequest,
            })
          }}
        >
          Run all tests
        </VSCodeButton>
      </div>
      <div className="flex flex-col py-4 divide-y gap-y-4 divide-vscode-descriptionForeground">
        <pre>{JSON.stringify(input_json_schema, null, 2)}</pre>
        {test_cases.map((test_case) => (
          <div key={test_case.name.value}>
            <div className="flex flex-row items-center gap-x-1">
              <Button
                variant={'ghost'}
                size={'icon'}
                className="p-1 w-fit h-fit"
                onClick={() => {
                  const runTestRequest: TestRequest = {
                    functions: [
                      {
                        name: func.name.value,
                        tests: [
                          {
                            name: test_case.name.value,
                            params: getTestParams(test_case),
                            impls: impl ? [impl.name.value] : [],
                          },
                        ],
                      },
                    ],
                  }
                  vscode.postMessage({
                    command: 'runTest',
                    data: runTestRequest,
                  })
                }}
              >
                <Play size={10} />
              </Button>
              <div>{test_case.name.value}</div>
              <EditTestCaseForm
                testCase={test_case}
                schema={input_json_schema}
                func={func}
                getTestParams={getTestParams}
              />
            </div>
            <TestCaseCard content={test_case.content} testCaseName={test_case.name.value} />
          </div>
        ))}
      </div>
    </>
  )

  // return (
  //   <div className="flex flex-col">
  //     {func.input.arg_type === 'positional' ? (
  //       <PositionalTestCase input={func.input.type} content={test_case?.content} />
  //     ) : (
  //       <NamedTestCase values={func.input.values} content={test_case?.content} />
  //     )}
  //   </div>
  // )
}

const EditTestCaseForm = ({
  testCase,
  schema,
  func,
  getTestParams,
}: {
  func: Func
  testCase: Func['test_cases'][0]
  schema: any
  getTestParams: (testCase: Func['test_cases'][0]) => void
}) => {
  const { root_path } = useContext(ASTContext)

  // TODO, actually fix this for named args
  const formData = useMemo(() => {
    try {
      return JSON.parse(testCase.content)
    } catch (e) {
      console.log('Error parsing data\n' + testCase.content, e)
      return testCase.content
    }
  }, [testCase.content])

  return (
    <Dialog>
      <DialogTrigger asChild={true}>
        <Button variant={'ghost'} size="icon" className="p-1 w-fit h-fit">
          <Edit2 className="w-3 h-3 text-vscode-descriptionForeground" />
        </Button>
      </DialogTrigger>
      <DialogContent className="max-h-screen overflow-y-scroll bg-vscode-editorWidget-background border-vscode-descriptionForeground">
        <Form
          schema={schema}
          formData={formData}
          validator={validator}
          uiSchema={uiSchema}
          // widgets={widgets}
          templates={{
            BaseInputTemplate: MyBaseInputTemplate,
            FieldTemplate: MyFieldTemplate,
            ObjectFieldTemplate: MyObjectTemplate,
            ButtonTemplates: {
              AddButton,
              RemoveButton,
            },
            ArrayFieldTitleTemplate: ArrayFieldTitleTemplate,
            ArrayFieldItemTemplate: ArrayFieldItemTemplate,
          }}
          onSubmit={(data) => {
            vscode.postMessage({
              command: 'saveTest',
              data: {
                root_path,
                funcName: func.name.value,
                testCaseName: testCase.name, // a stringspan fyi
                params: getTestParams({
                  ...testCase,
                  content: JSON.stringify(data.formData, null, 2),
                }),
              },
            })
          }}
        />
      </DialogContent>
    </Dialog>
  )
}

const TestCaseCard: React.FC<{ testCaseName: string; content: string }> = ({ testCaseName, content }) => {
  return (
    <div className="flex flex-col gap-2 text-xs text-vscode-descriptionForeground">
      <div>
        {content.substring(0, 200)}
        {content.length > 200 && '...'}
      </div>
    </div>
  )
}

const PositionalTestCase: React.FC<{ input: string; content: string | undefined }> = ({ input, content }) => {
  const [singleArgValue, setSingleArgValue] = useState(content ?? '')

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-row items-baseline gap-1">
        <span>input</span>
        <div className="text-xs italic">
          <TypeComponent typeString={input} />
        </div>
      </div>

      <VSCodeTextArea
        placeholder="Enter the input as json like { 'hello': 'world' } or a string"
        className="w-full"
        resize="vertical"
        value={singleArgValue}
        onInput={(e: any) => {
          setSingleArgValue(e.target.value)
        }}
      />
      <TestButtons
        data={() => ({
          type: 'positional',
          value: singleArgValue,
        })}
      />
    </div>
  )
}

const NamedTestCase: React.FC<{ values: { name: StringSpan; type: string }[]; content: string | any | undefined }> = ({
  values,
  content: raw_content,
}) => {
  const [content, setContent] = useState(new Map<string, string>())

  useEffect(() => {
    if (!raw_content) setContent(new Map<string, string>())

    try {
      const parsed = JSON.parse(raw_content)
      if (typeof parsed === 'object') {
        // As a key value pair
        setContent(new Map(Object.entries(parsed).map(([k, v]) => [k, JSON.stringify(v, null, 2)])))
      }
    } catch (e) {}
  }, [raw_content])

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-1">
        {typeof content === 'string' ? (
          <div className="text-red-500">
            Something went wrong. Expect a json, but got invalid content. Open the file and fix it manually.
          </div>
        ) : (
          <>
            {values.map(({ name, type }) => (
              <div key={name.value} className="flex flex-col gap-1">
                <div className="flex flex-row items-baseline gap-1">
                  <span>{name.value}</span>
                  <div className="text-xs italic">
                    <TypeComponent typeString={type} />
                  </div>
                </div>
                <VSCodeTextArea
                  placeholder="Enter the input as json like { 'hello': 'world' } or a string"
                  className="w-full"
                  resize="vertical"
                  value={content.get(name.value) ?? ''}
                  onInput={(e: any) => {
                    setContent((c) => {
                      c.set(name.value, e.target.value)
                      return c
                    })
                  }}
                />
              </div>
            ))}
            <TestButtons
              data={() => ({
                type: 'named',
                value: values.map(({ name }) => ({
                  name: name.value,
                  value: content.get(name.value) ?? '',
                })),
              })}
            />
          </>
        )}
      </div>
    </div>
  )
}

const TestButtons: React.FC<{ data: () => TestRequest['functions'][0]['tests'][0]['params'] }> = ({ data }) => {
  const {
    func: { name: func_name, impls } = {},
    impl: { name } = {},
    test_case: { name: testCaseName } = {},
  } = useSelections()

  if (!name || !impls || !func_name) return <i>No impls for this function</i>

  return (
    <div className="flex flex-row justify-between">
      <div className="flex flex-row gap-2">
        <RunImplButton
          funcName={func_name.value}
          testCaseName={testCaseName?.value ?? 'playground'}
          implName={name.value}
          data={data}
        />
        {impls.length > 1 && (
          <RunAllImplsButton
            funcName={func_name.value}
            testCaseName={testCaseName?.value ?? 'playground'}
            impls={impls.map((i) => i.name.value)}
            data={data}
          />
        )}
      </div>
      <div>
        <SaveButton funcName={func_name.value} testCaseName={testCaseName} data={data} />
      </div>
    </div>
  )
}

const SaveButton: React.FC<{
  funcName: string
  testCaseName: StringSpan | undefined
  data: () => TestRequest['functions'][0]['tests'][0]['params']
}> = ({ funcName, testCaseName, data }) => {
  const { root_path } = useContext(ASTContext)
  return (
    <VSCodeButton
      className="flex justify-end"
      onClick={() => {
        vscode.postMessage({
          command: 'saveTest',
          data: {
            root_path,
            funcName,
            testCaseName,
            params: data(),
          },
        })
      }}
    >
      Save
    </VSCodeButton>
  )
}

const RunAllImplsButton: React.FC<{
  funcName: string
  testCaseName: string
  impls: string[]
  data: () => TestRequest['functions'][0]['tests'][0]['params']
}> = ({ funcName, testCaseName, impls, data }) => {
  return (
    <VSCodeButton
      className="flex justify-end"
      onClick={() => {
        const runTestRequest: TestRequest = {
          functions: [
            {
              name: funcName,
              tests: [
                {
                  name: testCaseName,
                  params: data(),
                  impls,
                },
              ],
            },
          ],
        }
        vscode.postMessage({
          command: 'runTest',
          data: runTestRequest,
        })
      }}
    >
      Run all impls
    </VSCodeButton>
  )
}

const RunImplButton: React.FC<{
  funcName: string
  testCaseName: string
  implName: string
  data: () => TestRequest['functions'][0]['tests'][0]['params']
}> = ({ funcName, testCaseName, implName, data }) => {
  return (
    <VSCodeButton
      className="flex justify-end"
      onClick={() => {
        const runTestRequest: TestRequest = {
          functions: [
            {
              name: funcName,
              tests: [
                {
                  name: testCaseName,
                  params: data(),
                  impls: [implName],
                },
              ],
            },
          ],
        }
        vscode.postMessage({
          command: 'runTest',
          data: runTestRequest,
        })
      }}
    >
      Run impl: {implName}
    </VSCodeButton>
  )
}

export default TestCasePanel
