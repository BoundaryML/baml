import { VSCodeDropdown, VSCodeLink, VSCodeOption } from '@vscode/webview-ui-toolkit/react'
import { useSelections } from './hooks'
import React, { useContext, useState } from 'react'
import { ASTContext } from './ASTProvider'
import { vscode } from '../utils/vscode'
import Link from './Link'
import TypeComponent from './TypeComponent'
import { ProjectToggle } from './ProjectPanel'
import { Command, CommandInput, CommandEmpty, CommandGroup, CommandItem, CommandList } from '../components/ui/command'
import { Popover, PopoverTrigger, PopoverContent } from '../components/ui/popover'
import { cn } from '../lib/utils'
import { Button } from '../components/ui/button'
import { ChevronsUpDown, Check } from 'lucide-react'
import SearchBarWithSelector from '../lib/searchbar'
import { SFunction } from '@baml/common'
import { useAtom, useAtomValue } from 'jotai'
import { availableFunctionsAtom, selectedFunctionAtom } from '../baml_wasm_web/EventListener'

const FunctionDropdown: React.FC = () => {
  const [open, setOpen] = useState(false)
  const functions = useAtomValue(availableFunctionsAtom)
  const [selected, setSelected] = useAtom(selectedFunctionAtom)

  const functionName = selected?.name

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          aria-expanded={open}
          className="max-w-[300px] justify-between flex hover:bg-vscode-editorSuggestWidget-selectedBackground hover:text-foreground"
        >
          <span className="-ml-2 w-full text-left truncate">{functionName ?? 'Select a function...'}</span>
          <ChevronsUpDown className="ml-2 w-4 h-4 opacity-50 shrink-0" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-1/3 min-w-[400px] p-0">
        <SearchBarWithSelector
          options={functions.map((func) => ({
            value: func.name,
            label: func.test_cases.length > 0 ? `${func.name} (${func.test_cases.length})` : undefined,
          }))}
          onChange={(value) => {
            setSelected(value)
            setOpen(false)
          }}
        />
      </PopoverContent>
    </Popover>
  )
}

export const FunctionArgs: React.FC<{ func: SFunction }> = ({ func }) => {
  if (func.input.arg_type === 'positional') {
    return (
      <div className="flex flex-row gap-1 whitespace-nowrap">
        arg: <TypeComponent typeString={func.input.type} />
      </div>
    )
  }

  const args = func.input.values
  return (
    <div className="flex flex-wrap gap-1">
      {Array.from(args.entries()).map(([i, v]) => (
        <div key={v.name.value} className="whitespace-nowrap">
          {v.name.value}: <TypeComponent typeString={v.type} /> {i < args.length - 1 && ','}
        </div>
      ))}
    </div>
  )
}

export const FunctionSelector: React.FC = () => {
  return (
    <div className="flex flex-col gap-1 items-start">
      <div className="flex flex-row gap-1 items-center">
        <ProjectToggle />

        <FunctionDropdown />
        {/* <span className="font-light">Function</span>
        <VSCodeDropdown
          value={func?.name.value ?? '<not-picked>'}
          onChange={(event) =>
            setSelection(
              undefined,
              (event as React.FormEvent<HTMLSelectElement>).currentTarget.value,
              undefined,
              undefined,
              undefined,
            )
          }
        >
          {function_names.map((func) => (
            <VSCodeOption key={func} value={func}>
              {func}
            </VSCodeOption>
          ))}
        </VSCodeDropdown> */}
      </div>
      {/* {func && (
        <div className="flex flex-row gap-0 items-center pl-2 text-xs whitespace-nowrap text-vscode-descriptionForeground">
          <Link item={func.name} />
          {'('}
          <FunctionArgs func={func} /> {') → '}{' '}
          {func.output.arg_type === 'positional' && <TypeComponent typeString={func.output.type} />}
        </div>
      )} */}
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
          setSelection(undefined, undefined, undefined, value, undefined)
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
