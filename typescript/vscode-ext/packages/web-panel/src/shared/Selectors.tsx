import { VSCodeDropdown, VSCodeLink, VSCodeOption } from '@vscode/webview-ui-toolkit/react'
import { useSelections } from './hooks'
import { useContext } from 'react'
import { ASTContext } from './ASTProvider'
import { vscode } from '@/utils/vscode'
import Link from './Link'

export const FunctionSelector: React.FC = () => {
  const {
    db: { functions },
    setSelection,
  } = useContext(ASTContext)
  const { func: { name } = {} } = useSelections()
  const function_names = functions.map((func) => func.name.value)

  return (
    <>
      <VSCodeDropdown
        value={name?.value ?? '<not-picked>'}
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
      {name && <Link item={name} display="Open File" />}
    </>
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
