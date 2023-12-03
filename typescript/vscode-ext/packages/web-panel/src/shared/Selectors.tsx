import { VSCodeDropdown, VSCodeLink, VSCodeOption } from '@vscode/webview-ui-toolkit/react'
import { useSelections } from './hooks'
import { useContext } from 'react'
import { ASTContext } from './ASTProvider'
import { vscode } from '@/utils/vscode'
import Link from './Link'
import TypeComponent from './TypeComponent'

export const FunctionSelector: React.FC = () => {
  const {
    db: { functions },
    setSelection,
  } = useContext(ASTContext)
  const { func } = useSelections()
  const function_names = functions.map((func) => func.name.value)

  return (
    <div className="flex flex-col gap-1 items-start">
      <div className="flex flex-row gap-1 items-center">
        <span className="font-light">Function</span>
        <VSCodeDropdown
          value={func?.name.value ?? '<not-picked>'}
          onChange={(event) =>
            setSelection((event as React.FormEvent<HTMLSelectElement>).currentTarget.value, undefined, undefined)
          }
        >
          {function_names.map((func) => (
            <VSCodeOption key={func} value={func}>
              {func}
            </VSCodeOption>
          ))}
        </VSCodeDropdown>
      </div>
      {func && (
        <div className="flex flex-row gap-1 items-center">
          <Link item={func.name} />(
          {func.input.arg_type === 'positional' ? (
            <div className="flex flex-row gap-1">
              arg: <TypeComponent typeString={func.input.type} />
            </div>
          ) : (
            <div className="flex flex-row gap-1">
              {func.input.values.map((v) => (
                <div key={v.name.value}>
                  {v.name.value}: <TypeComponent typeString={v.type} />,
                </div>
              ))}
            </div>
          )}
          ) {'->'} {func.output.arg_type === 'positional' && <TypeComponent typeString={func.output.type} />}
        </div>
      )}
    </div>
  )
}

export const TestCaseSelector: React.FC = () => {
  const PLACEHOLDER = '<new>'
  const { setSelection } = useContext(ASTContext)
  const { func, test_case: { name } = {} } = useSelections()
  const test_cases = func?.test_cases.map((cases) => cases.name.value) ?? []

  if (!func) return null

  return (
    <>
      <VSCodeDropdown
        value={name?.value ?? PLACEHOLDER}
        onChange={(event) => {
          let value = (event as React.FormEvent<HTMLSelectElement>).currentTarget.value
          setSelection(undefined, undefined, value)
        }}
      >
        {test_cases.map((cases, index) => (
          <VSCodeOption key={index} value={cases}>
            {cases}
          </VSCodeOption>
        ))}
        <VSCodeOption value={PLACEHOLDER}>{PLACEHOLDER}</VSCodeOption>
      </VSCodeDropdown>
      {name && <Link item={name} display="Open File" />}
    </>
  )
}
