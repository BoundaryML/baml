'use client'

import React, { useState } from 'react'
import { Popover, PopoverTrigger, PopoverContent } from '../components/ui/popover'
import { Button } from '../components/ui/button'
import { ChevronsUpDown } from 'lucide-react'
import SearchBarWithSelector from '../lib/searchbar'
import { useAtom, useAtomValue } from 'jotai'
import { availableFunctionsAtom, selectedFunctionAtom, selectedRtFunctionAtom } from './EventListener'
import Link from '../shared/Link'

const FunctionDropdown: React.FC = () => {
  const [open, setOpen] = useState(false)
  const functions = useAtomValue(availableFunctionsAtom);
  const [selectedFunction, setSelectedFunction] = useAtom(selectedFunctionAtom);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          aria-expanded={open}
          className="max-w-[300px] justify-between flex hover:bg-vscode-editorSuggestWidget-selectedBackground hover:text-foreground"
        >
          <span className="w-full -ml-2 text-left truncate">{selectedFunction ?? 'Select a function...'}</span>
          <ChevronsUpDown className="w-4 h-4 ml-2 opacity-50 shrink-0" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-1/3 min-w-[400px] p-0">
        <SearchBarWithSelector
          options={functions.map((func) => ({ value: func.name }))}
          onChange={(value) => {
            setSelectedFunction(value)
            setOpen(false)
          }}
        />
      </PopoverContent>
    </Popover>
  )
}

export const FunctionSelector: React.FC = () => {
  const selectedFunc = useAtomValue(selectedRtFunctionAtom);
  return (
    <div className="flex flex-col items-start gap-1">
      <div className="flex flex-row items-center gap-1">
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
      {selectedFunc && (
        <div className="flex flex-row items-center gap-0 pl-2 text-xs text-vscode-descriptionForeground whitespace-nowrap">
          <Link item={{
            end: 0,
            start: 0,
            source_file: '',
            value: selectedFunc.name
          }} />
          {'('}
          {/* <FunctionArgs func={func} /> {') → '}{' '}
          {func.output.arg_type === 'positional' && <TypeComponent typeString={func.output.type} />} */}
        </div>
      )}
    </div>
  )
}
