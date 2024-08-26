import { useAtom, useAtomValue } from 'jotai'
import { ChevronDown, ChevronRight, Compass } from 'lucide-react'
import type React from 'react'
import { useState } from 'react'
import {
  availableFunctionsAtom,
  orchIndexAtom,
  selectedFunctionAtom,
  selectedTestCaseAtom,
  availableClientsAtom,
  currentClientsAtom,
} from '../baml_wasm_web/EventListener'
import { Button } from '../components/ui/button'
import { Popover, PopoverContent, PopoverTrigger } from '../components/ui/popover'
import SearchBarWithSelector from '../lib/searchbar'
import Link from './Link'
import { Dialog, DialogContent, DialogTrigger } from '../components/ui/dialog'
import { Snippets } from './Snippets'

const ClientHeader: React.FC = () => {
  const orchIndex = useAtomValue(orchIndexAtom)

  const clientsArray = useAtomValue(currentClientsAtom)
  const currentClient = clientsArray[orchIndex]
  return (
    <div className='flex flex-col-reverse items-start gap-0.5'>
      <span className='pl-2 text-xs text-muted-foreground flex flex-row flex-wrap items-center gap-0.5'>
        {clientsArray.length > 1 && `Attempt ${orchIndex} in Client Graph`}
      </span>
      <div className='max-w-[300px] justify-start items-center flex hover:bg-vscode-button-hoverBackground h-fit rounded-md text-vscode-foreground cursor-pointer'>
        <span className='px-2 py-1 w-full text-left truncate'>{currentClient}</span>
      </div>
    </div>
  )
}

const FunctionDropdown: React.FC = () => {
  const [open, setOpen] = useState(false)
  const functions = useAtomValue(availableFunctionsAtom)
  const [selected, setSelected] = useAtom(selectedFunctionAtom)

  const functionName = selected?.name

  if (functions.length === 0) {
    return <>Create a function</>
  }

  const isNextJS = (window as any).next?.version!

  return (
    <div className='flex flex-col-reverse items-start gap-0.5'>
      {!isNextJS && (
        <span className='pl-2 text-xs text-muted-foreground flex flex-row flex-wrap items-center gap-0.5'>
          {/* Function */}
          {selected && <JumpToFunction />}
        </span>
      )}
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <div className='max-w-[300px] justify-start items-center flex hover:bg-vscode-button-hoverBackground h-fit rounded-md text-vscode-foreground cursor-pointer'>
            <span className='px-2 py-1 w-full text-left truncate'>{functionName ?? 'Select a function...'}</span>
            <ChevronDown className='ml-1 w-4 h-4 opacity-50 shrink-0' />
          </div>
        </PopoverTrigger>
        <PopoverContent className='w-1/3 min-w-[400px] p-0'>
          <SearchBarWithSelector
            options={functions.map((func) => ({
              value: func.name,
              label: func.test_cases.length > 0 ? `${func.name} (${func.test_cases.length} tests)` : undefined,
              content: (
                <div className='flex flex-row gap-1 items-center'>
                  <span>{func.signature}</span>
                </div>
              ),
            }))}
            onChange={(value) => {
              setSelected(value)
              setOpen(false)
            }}
          />
        </PopoverContent>
      </Popover>
    </div>
  )
}

const TestDropdown: React.FC = () => {
  const [open, setOpen] = useState(false)
  const tests = useAtomValue(selectedFunctionAtom)?.test_cases
  const [selected, setSelected] = useAtom(selectedTestCaseAtom)

  if (tests === undefined) {
    return null
  }

  if (tests.length === 0) {
    return <>Create a test</>
  }

  if (!selected) {
    return <>Select a test...</>
  }
  const isNextJS = (window as any).next?.version!

  return (
    <div className='flex flex-col-reverse items-start gap-0.5'>
      {!isNextJS && (
        <span className='flex flex-row flex-wrap gap-1 items-center pl-2 text-xs text-muted-foreground'>
          {/* Test */}
          {selected && <JumpToTestCase />}
        </span>
      )}

      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <div className='max-w-[300px] justify-between items-center flex hover:bg-vscode-button-hoverBackground h-fit rounded-md text-vscode-foreground cursor-pointer'>
            <span className='px-2 py-1 w-full text-left truncate'>{selected.name}</span>
            <ChevronDown className='ml-1 w-4 h-4 opacity-50 shrink-0' />
          </div>
        </PopoverTrigger>
        <PopoverContent className='w-1/3 min-w-[400px] p-0'>
          <SearchBarWithSelector
            options={tests.map((test) => ({
              value: test.name,
              content: (
                <div className='flex flex-col gap-1 justify-start items-start'>
                  {test.inputs.map((i) => (
                    <div key={i.name} className='flex flex-row gap-1'>
                      <span>{i.name}</span>
                      <span className='max-w-[250px] truncate'>{i.value}</span>
                    </div>
                  ))}
                </div>
              ),
            }))}
            onChange={(value) => {
              setSelected(value)
              setOpen(false)
            }}
          />
        </PopoverContent>
      </Popover>
    </div>
  )
}

const JumpToFunction: React.FC = () => {
  const selected = useAtomValue(selectedFunctionAtom)

  if (!selected) {
    return null
  }

  return (
    <Link
      item={{
        start: selected.span.start,
        end: selected.span.end,
        source_file: selected.span.file_path,
        value: `${selected.span.file_path.split('/').pop() ?? '<file>.baml'}:${selected.span.start_line + 1}`,
      }}
      display='Open file'
      className='py-0 text-xs text-muted-foreground decoration-0'
    />
  )
}

const JumpToTestCase: React.FC = () => {
  const selected = useAtomValue(selectedTestCaseAtom)

  if (!selected) {
    return null
  }

  return (
    <Link
      item={{
        start: selected.span.start,
        end: selected.span.end,
        source_file: selected.span.file_path,
        value: `${selected.span.file_path.split('/').pop() ?? '<file>.baml'}:${selected.span.start_line + 1}`,
      }}
      display='Open file'
      className='text-xs text-muted-foreground decoration-0'
    />
  )
}

export const ViewSelector: React.FC = () => {
  return (
    <div className='flex overflow-x-auto flex-row justify-between w-full'>
      <div className='flex overflow-x-auto flex-row gap-4 items-center px-2 py-1'>
        <FunctionDropdown />
        <div>
          <ChevronRight className='w-4 h-4' />
        </div>
        <TestDropdown />
        <ChevronRight className='w-4, h-4' />
        <ClientHeader />
      </div>
      <div className='flex absolute right-1 top-2 z-10 flex-row gap-1 justify-center items-center text-end'>
        <Dialog>
          <DialogTrigger asChild>
            <Button
              variant={'ghost'}
              className='flex flex-row gap-x-2 items-center px-2 py-1 mr-2 text-sm text-white whitespace-pre-wrap bg-indigo-600 hover:bg-indigo-500 h-fit'
            >
              <Compass size={16} strokeWidth={2} />
              <span className='whitespace-nowrap'>Docs</span>
            </Button>
          </DialogTrigger>
          <DialogContent className='min-w-full h-full fullWidth border-zinc-900 bg-zinc-900'>
            <Snippets />
          </DialogContent>
        </Dialog>
      </div>
    </div>

    // <Breadcrumb className='px-2 py-1'>
    //   <BreadcrumbList>
    //     <BreadcrumbItem>
    //     </BreadcrumbItem>
    //     <BreadcrumbSeparator />
    //     <BreadcrumbItem>
    //       <TestDropdown />
    //     </BreadcrumbItem>
    //   </BreadcrumbList>
    // </Breadcrumb>
  )
}

// export const FunctionSelector: React.FC = () => {
//   return (
//     <div className='flex flex-col gap-1 items-start'>
//       <div className='flex flex-row gap-1 items-center'>
//         {/* <ProjectToggle /> */}

//         <FunctionDropdown />
//         {/* <span className="font-light">Function</span>
//         <VSCodeDropdown
//           value={func?.name.value ?? '<not-picked>'}
//           onChange={(event) =>
//             setSelection(
//               undefined,
//               (event as React.FormEvent<HTMLSelectElement>).currentTarget.value,
//               undefined,
//               undefined,
//               undefined,
//             )
//           }
//         >
//           {function_names.map((func) => (
//             <VSCodeOption key={func} value={func}>
//               {func}
//             </VSCodeOption>
//           ))}
//         </VSCodeDropdown> */}
//       </div>
//       {/* {func && (
//         <div className="flex flex-row gap-0 items-center pl-2 text-xs whitespace-nowrap text-vscode-descriptionForeground">
//           <Link item={func.name} />
//           {'('}
//           <FunctionArgs func={func} /> {') → '}{' '}
//           {func.output.arg_type === 'positional' && <TypeComponent typeString={func.output.type} />}
//         </div>
//       )} */}
//     </div>
//   )
// }
