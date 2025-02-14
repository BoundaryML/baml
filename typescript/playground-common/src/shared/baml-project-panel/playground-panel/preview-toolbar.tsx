'use client'

import { Button } from '@/components/ui/button'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { atom, useAtom, useAtomValue, useSetAtom } from 'jotai'
import { Braces, Bug, BugIcon, ChevronDown, Copy, FileJson, PlayCircle, Settings, Workflow } from 'lucide-react'
import React from 'react'
import { ThemeToggle } from '../theme/ThemeToggle'
import { areTestsRunningAtom, selectedItemAtom, showEnvDialogAtom } from './atoms'
import { FunctionTestName } from './function-test-name'
import { useRunTests } from './prompt-preview/test-panel/test-runner'
import { cn } from '@/lib/utils'
import { areEnvVarsMissingAtom } from './atoms'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { TooltipProvider } from '@/components/ui/tooltip'
import { renderedPromptAtom } from './prompt-preview/prompt-preview-content'
export const renderModeAtom = atom<'prompt' | 'curl' | 'tokens'>('prompt')

const RunButton: React.FC = () => {
  const { setRunningTests } = useRunTests()
  const isRunning = useAtomValue(areTestsRunningAtom)
  const selected = useAtomValue(selectedItemAtom)
  return (
    <Button
      variant='default'
      size='sm'
      className='items-center px-2 space-x-2 h-7 text-sm text-white bg-purple-500 hover:bg-purple-700 disabled:bg-muted disabled:text-muted-foreground dark:bg-purple-600 dark:text-foreground dark:hover:bg-purple-800'
      disabled={isRunning || selected === undefined}
      onClick={() => {
        if (selected) {
          void setRunningTests([{ functionName: selected[0], testName: selected[1] }])
        }
      }}
    >
      <PlayCircle className='mr-0 w-4 h-4' />
      <div className='text-xs'>Run {selected ? selected[1] : ''}</div>
    </Button>
  )
}

export const isClientCallGraphEnabledAtom = atom(false)

export default function Component() {
  const [renderMode, setRenderMode] = useAtom(renderModeAtom)
  const selections = useAtomValue(selectedItemAtom)
  const setShowEnvDialog = useSetAtom(showEnvDialogAtom)

  const options: {
    label: string
    icon: React.FC<React.SVGProps<SVGSVGElement>>
    value: 'prompt' | 'curl' | 'tokens'
  }[] = [
    { label: 'Prompt Preview', icon: FileJson, value: 'prompt' },
    { label: 'Token Visualization', icon: Braces, value: 'tokens' },
    { label: 'Raw cURL', icon: Bug, value: 'curl' },
  ]

  const areEnvVarsMissing = useAtomValue(areEnvVarsMissingAtom)
  const [isClientCallGraphEnabled, setIsClientCallGraphEnabled] = useAtom(isClientCallGraphEnabledAtom)
  const renderedPrompt = useAtomValue(renderedPromptAtom)
  const [showCopied, setShowCopied] = React.useState(false)

  const selectedOption = options.find((opt) => opt.value === renderMode)

  const SelectedIcon = selectedOption?.icon || FileJson

  const handleCopy = () => {
    if (!renderedPrompt) return
    navigator.clipboard.writeText(
      renderedPrompt
        .as_chat()
        ?.map((msg) => `${msg.role}:\n${msg.parts.map((part) => part.as_text()).join('\n')}`)
        .join('\n\n') ?? '',
    )
    setShowCopied(true)
    setTimeout(() => setShowCopied(false), 1500)
  }

  return (
    <div className='flex flex-col gap-1'>
      <div
        className={cn('flex flex-row gap-1 items-center', selections === undefined ? 'justify-end' : 'justify-start')}
      >
        {selections !== undefined && <FunctionTestName functionName={selections[0]} testName={selections[1]} />}
        <Button
          variant='ghost'
          size='sm'
          className='flex gap-2 items-center text-muted-foreground/70'
          onClick={() => setShowEnvDialog(true)}
        >
          <div className='relative'>
            <Settings className='w-4 h-4 text-muted-foreground' />
            {areEnvVarsMissing && <div className='absolute -top-1 -right-1 w-2 h-2 bg-orange-500 rounded-full' />}
          </div>
          <span>API Keys</span>
        </Button>
        <ThemeToggle />
      </div>

      <div className='flex items-center space-x-4 w-full'>
        <RunButton />
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant='outline'
              size='sm'
              className='h-8 border-border bg-background hover:bg-accent hover:text-accent-foreground'
            >
              <SelectedIcon className='mr-2 w-4 h-4' />
              {selectedOption?.label}
              <ChevronDown className='ml-2 w-4 h-4 opacity-50' />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align='start' className='border-border bg-background'>
            {options.map((option) => (
              <DropdownMenuItem
                key={option.label}
                onSelect={() => setRenderMode(option.value)}
                className='hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground'
              >
                <option.icon className='mr-2 w-4 h-4' />
                {option.label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        <TooltipProvider>
          <Tooltip delayDuration={100}>
            <TooltipTrigger>
              <Button
                variant='ghost'
                size='sm'
                className={cn(
                  isClientCallGraphEnabled ? 'text-purple-500 bg-muted hover:text-purple-500' : 'hover:text-purple-500',
                )}
                onClick={() => setIsClientCallGraphEnabled(!isClientCallGraphEnabled)}
              >
                <Workflow className='w-4 h-4' />
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              <p>Show LLM Client Call Graph</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
        <TooltipProvider>
          <Tooltip delayDuration={100}>
            <TooltipTrigger>
              <Button variant='ghost' size='sm' className={cn('hover:text-purple-500')} onClick={handleCopy}>
                {showCopied ? 'Copied!' : <Copy className='w-4 h-4' />}
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              <p>Copy Prompt</p>
            </TooltipContent>
          </Tooltip>
        </TooltipProvider>
      </div>
    </div>
  )
}
