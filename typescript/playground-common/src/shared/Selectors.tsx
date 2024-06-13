import type { SFunction } from '@baml/common'
import { VSCodeDropdown, VSCodeLink, VSCodeOption } from '@vscode/webview-ui-toolkit/react'
import { useAtom, useAtomValue } from 'jotai'
import { Check, ChevronsUpDown } from 'lucide-react'
import type React from 'react'
import { useContext, useState } from 'react'
import { availableFunctionsAtom, selectedFunctionAtom } from '../baml_wasm_web/EventListener'
import { Button } from '../components/ui/button'
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from '../components/ui/command'
import { Popover, PopoverContent, PopoverTrigger } from '../components/ui/popover'
import SearchBarWithSelector from '../lib/searchbar'

import Link from './Link'

const FunctionDropdown: React.FC = () => {
  const [open, setOpen] = useState(false)
  const functions = useAtomValue(availableFunctionsAtom)
  const [selected, setSelected] = useAtom(selectedFunctionAtom)

  const functionName = selected?.name

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant='ghost'
          aria-expanded={open}
          className='max-w-[300px] justify-between flex hover:bg-vscode-editorSuggestWidget-selectedBackground hover:text-foreground'
        >
          <span className='w-full -ml-1 text-left truncate'>{functionName ?? 'Select a function...'}</span>
          <ChevronsUpDown className='w-4 h-4 ml-1 opacity-50 shrink-0' />
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-1/3 min-w-[400px] p-0'>
        <SearchBarWithSelector
          options={functions.map((func) => ({
            value: func.name,
            label: func.test_cases.length > 0 ? `${func.name} (${func.test_cases.length} tests)` : undefined,
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

export const FunctionSelector: React.FC = () => {
  return (
    <div className='flex flex-col items-start gap-1'>
      <div className='flex flex-row items-center gap-1'>
        {/* <ProjectToggle /> */}

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
        <div className="flex flex-row items-center gap-0 pl-2 text-xs whitespace-nowrap text-vscode-descriptionForeground">
          <Link item={func.name} />
          {'('}
          <FunctionArgs func={func} /> {') → '}{' '}
          {func.output.arg_type === 'positional' && <TypeComponent typeString={func.output.type} />}
        </div>
      )} */}
    </div>
  )
}
