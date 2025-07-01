'use client'

import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  DialogFooter,
  DialogClose,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { useToast } from '@/components/hooks/use-toast'
import { atom, useAtomValue, useSetAtom } from 'jotai'
import { AlertTriangle, Check, Circle, CircleDot, Eye, EyeOff, PlusCircle, Settings2, Trash2 } from 'lucide-react'
import { QuestionMarkCircledIcon } from '@radix-ui/react-icons'
import { useState, useRef, useEffect } from 'react'
import { envVarsAtom, requiredEnvVarsAtom, proxyUrlAtom, userEnvVarsAtom } from '../../atoms'
import { Textarea } from '@/components/ui/textarea'
import { Save, FileText } from 'lucide-react'
import { Label } from '@/components/ui/label'
import { Checkbox } from '@/components/ui/checkbox'
import { vscode } from '../../vscode'
import { sortBy } from 'lodash'
import { parse as parseDotenv } from 'dotenv'
import { motion } from 'motion/react'
import { bamlConfig, type BamlConfigAtom } from '../../../../baml_wasm_web/bamlConfig'

const envVarVisibilityAtom = atom<Record<string, boolean>>({})

const REQUIRED_ENV_VAR_UNSET_WARNING = 'Your BAML clients may fail if this is not set'

interface EnvVarEntry {
  key: string
  value: string | undefined
  required: boolean
  hidden: boolean
}

const renderedEnvVarsAtom = atom<EnvVarEntry[]>((get) => {
  const userEnvVars = get(userEnvVarsAtom) as Record<string, string>
  const requiredEnvVars = get(requiredEnvVarsAtom)
  const visibility = get(envVarVisibilityAtom)

  const vars: EnvVarEntry[] = Object.entries(userEnvVars).map(([key, value]) => ({
    key,
    value,
    required: requiredEnvVars.includes(key),
    hidden: visibility[key] !== false, // hidden by default unless explicitly set to false
  }))

  const missingVars = requiredEnvVars.filter((envVar) => !(envVar in userEnvVars))

  vars.push(
    ...missingVars.map((envVar) => ({
      key: envVar,
      value: undefined,
      required: true,
      hidden: visibility[envVar] !== false,
    })),
  )

  return sortBy(vars, [(v) => v.key])
})

const escapeValue = (value: string): string => {
  return value.replace(/[\n\r\t]/g, (match) => {
    switch (match) {
      case '\n':
        return '\\n'
      case '\r':
        return '\\r'
      case '\t':
        return '\\t'
      default:
        return match
    }
  })
}

const unescapeValue = (value: string): string => {
  return value.replace(/\\[nrt]/g, (match) => {
    switch (match) {
      case '\\n':
        return '\n'
      case '\\r':
        return '\r'
      case '\\t':
        return '\t'
      default:
        return match
    }
  })
}

function EnvVarStatus({ value, required }: { value?: string; required: boolean }) {
  if (!value || value === '') {
    return (
      <TooltipProvider delayDuration={300}>
        <Tooltip>
          <TooltipTrigger asChild>
            <AlertTriangle className='h-4 w-4 text-orange-500 flex-shrink-0' />
          </TooltipTrigger>
          <TooltipContent side='top' className='text-xs'>
            {value ? 'Click to edit' : REQUIRED_ENV_VAR_UNSET_WARNING}
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    )
  }

  if (required) {
    return (
      <TooltipProvider delayDuration={300}>
        <Tooltip>
          <TooltipTrigger asChild>
            <Check className='h-4 w-4 text-green-500 flex-shrink-0' />
          </TooltipTrigger>
          <TooltipContent side='top' className='text-xs'>
            Used by one of your BAML clients
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
    )
  }

  return <div />
}

export const EnvironmentVariablesPanel: React.FC = () => {
  const envVars = useAtomValue(renderedEnvVarsAtom)
  const setUserEnvVars = useSetAtom(userEnvVarsAtom)
  const setVisibility = useSetAtom(envVarVisibilityAtom)
  const currentUserEnvVars = useAtomValue(userEnvVarsAtom)
  const proxySettings = useAtomValue(proxyUrlAtom)
  const setBamlConfig = useSetAtom(bamlConfig)

  const [newKey, setNewKey] = useState('')
  const [newValue, setNewValue] = useState('')
  const [envFileContent, setEnvFileContent] = useState('')
  const { toast } = useToast()

  // Toggle visibility of an environment variable
  const toggleVisibility = (key: string) => {
    setVisibility((prev) => ({
      ...prev,
      [key]: !prev[key],
    }))
  }

  // Update an environment variable immediately
  const updateEnvVar = (key: string, value: string) => {
    const newVars = { ...currentUserEnvVars }
    newVars[key] = value
    setUserEnvVars(newVars)
  }

  // Delete an environment variable
  const deleteEnvVar = (key: string) => {
    const newVars = { ...currentUserEnvVars }
    delete newVars[key]
    setUserEnvVars(newVars)
  }

  // Add a new environment variable
  const addEnvVar = () => {
    if (newKey.trim() === '') return

    const newVars = { ...currentUserEnvVars }
    newVars[newKey] = newValue
    setUserEnvVars(newVars)

    // Reset form
    setNewKey('')
    setNewValue('')
  }

  // Parse and import environment variables from .env file
  const parseAndSaveEnvFile = () => {
    try {
      const parsed = parseDotenv(envFileContent)
      const newVars = { ...currentUserEnvVars }
      Object.entries(parsed).forEach(([key, value]) => {
        newVars[key] = value
      })
      setUserEnvVars(newVars)
      setEnvFileContent('')
      toast({
        title: 'Environment variables imported',
        description: `Successfully imported ${Object.keys(parsed).length} variables`,
      })
    } catch (error) {
      toast({
        title: 'Error parsing .env file',
        description: 'Please check the format of your .env file',
        variant: 'destructive',
      })
    }
  }

  return (
    <div className='p-2 space-y-2 text-sm'>
      <h3 className='flex gap-2 items-center font-medium text-muted-foreground'>
        <Settings2 className='w-4 h-4' />
        Environment Variables
      </h3>
      <div className='text-left text-muted-foreground'>
        <p>
          Set your own API Keys here.&nbsp;
          <a
            href='https://docs.boundaryml.com/ref/llm-client-providers/overview#fields'
            target='_blank'
            rel='noopener noreferrer'
            className='text-blue-500 hover:underline'
          >
            See supported LLMs
          </a>
        </p>
      </div>
      <div className='text-left text-muted-foreground'>
        <div className='flex gap-2 items-center'>
          <div className='flex gap-2 items-center'>
            <TooltipProvider delayDuration={300}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <QuestionMarkCircledIcon className='w-4 h-4' />
                </TooltipTrigger>
                <TooltipContent side='top' className='text-xs w-80'>
                  The BAML playground directly calls the LLM provider's API. Some providers make it difficult for
                  browsers to call their API due to CORS restrictions.
                  <br />
                  <br />
                  To get around this, the BAML VSCode extension includes a <b>localhost proxy</b> that sits between your
                  browser and the LLM provider's API.
                  <br />
                  <br />
                  <b>BAML MAKES NO NETWORK CALLS BEYOND THE LLM PROVIDER'S API YOU SPECIFY.</b>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
            <p>
              VSCode proxy is <b>{proxySettings.proxyEnabled ? 'enabled' : 'disabled'}</b>
            </p>
            <Checkbox
              checked={proxySettings.proxyEnabled}
              onCheckedChange={async (checked) => {
                try {
                  const response = await vscode.setProxySettings(!!checked)
                  // Update local config to reflect the change immediately
                  setBamlConfig((prev: BamlConfigAtom) => ({
                    ...prev,
                    config: {
                      ...prev.config,
                      enablePlaygroundProxy: response.enablePlaygroundProxy,
                    },
                  }))
                } catch (error) {
                  console.error('Failed to update proxy settings:', error)
                  toast({
                    title: 'Error updating proxy settings',
                    description: 'Please try again',
                    variant: 'destructive',
                  })
                }
              }}
            />
          </div>
          <p>{proxySettings.proxyUrl}</p>
        </div>
      </div>

      <div className='space-y-1'>
        <table className='w-full'>
          <tbody>
            {envVars
              .filter(({ key }) => key !== 'BOUNDARY_PROXY_URL')
              .map((env) => (
                <motion.tr
                  initial={{ opacity: 0, y: 2 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.05 }}
                  key={env.key}
                  className='relative hover:bg-accent/50 rounded-md'
                >
                  <td className='pl-2 pr-0.5 py-0.5'>
                    <div className='flex items-center gap-2 justify-between'>
                      <code className='font-mono text-xs text-muted-foreground'>{env.key}</code>
                      <EnvVarStatus value={env.value} required={env.required} />
                    </div>
                  </td>
                  <td className='px-0.5 py-0.5'>
                    <TooltipProvider key={env.key} delayDuration={300}>
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <Input
                            type={env.hidden ? 'password' : 'text'}
                            value={typeof env.value === 'string' ? escapeValue(env.value) : ''}
                            onChange={(e) => updateEnvVar(env.key, unescapeValue(e.target.value))}
                            className='h-6 text-xs font-mono placeholder:font-sans min-w-32'
                            placeholder={env.required && !env.value ? '<unset>' : undefined}
                            autoComplete='off'
                            data-1p-ignore
                          />
                        </TooltipTrigger>
                        <TooltipContent side='top' className='text-xs'>
                          {env.value ? 'Click to edit' : REQUIRED_ENV_VAR_UNSET_WARNING}
                        </TooltipContent>
                      </Tooltip>
                    </TooltipProvider>
                  </td>
                  <td className='pl-0.5 pr-2 py-0.5 text-right'>
                    <div className='flex gap-1 justify-end'>
                      <TooltipProvider delayDuration={300}>
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Button
                              variant='ghost'
                              size='sm'
                              className='p-0.5 w-5 h-5'
                              onClick={() => toggleVisibility(env.key)}
                            >
                              {env.hidden ? (
                                <EyeOff className='w-4 h-4 text-muted-foreground hover:text-primary' />
                              ) : (
                                <Eye className='w-4 h-4 text-muted-foreground hover:text-primary' />
                              )}
                            </Button>
                          </TooltipTrigger>
                          <TooltipContent side='top' className='text-xs'>
                            {env.hidden ? 'Click to show value' : 'Click to hide value'}
                          </TooltipContent>
                        </Tooltip>
                      </TooltipProvider>
                      <TooltipProvider delayDuration={300}>
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Button
                              variant='ghost'
                              size='sm'
                              className='p-0.5 w-5 h-5'
                              onClick={() => deleteEnvVar(env.key)}
                            >
                              <Trash2 className='w-4 h-4 text-muted-foreground hover:text-destructive' />
                            </Button>
                          </TooltipTrigger>
                          <TooltipContent side='top' className='text-xs'>
                            Delete environment variable
                          </TooltipContent>
                        </Tooltip>
                      </TooltipProvider>
                    </div>
                  </td>
                </motion.tr>
              ))}
            <motion.tr
              initial={{ opacity: 0, y: 2 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.05 }}
              className='rounded-md'
            >
              <td className='pl-2 pr-0.5 py-0.5'>
                <Input
                  value={newKey}
                  onChange={(e) => setNewKey(e.target.value)}
                  placeholder='New environment variable'
                  className='h-6 text-xs font-mono placeholder:font-sans'
                />
              </td>
              <td className='px-0.5 py-0.5'>
                <Input
                  value={newValue}
                  onChange={(e) => setNewValue(e.target.value)}
                  placeholder='Value'
                  className='h-6 text-xs font-mono placeholder:font-sans'
                />
              </td>
              <td className='pl-0.5 pr-0.5 py-0.5'>
                <Button size='sm' variant='outline' onClick={addEnvVar} className='h-8'>
                  <PlusCircle className='mr-2 w-4 h-4' />
                  Add
                </Button>
              </td>
            </motion.tr>
          </tbody>
        </table>
      </div>

      <Dialog>
        <DialogTrigger asChild>
          <Button variant='outline' size='sm' className='w-full mt-2'>
            <FileText className='h-4 w-4 mr-2' />
            Import from .env
          </Button>
        </DialogTrigger>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Import from .env file</DialogTitle>
          </DialogHeader>
          <div className='py-4'>
            <Label htmlFor='env-file'>Paste your .env file content below:</Label>
            <Textarea
              id='env-file'
              className='min-h-[200px] mt-2 font-mono text-xs'
              placeholder='KEY=value'
              value={envFileContent}
              onChange={(e) => setEnvFileContent(e.target.value)}
            />
          </div>
          <DialogFooter>
            <DialogClose asChild>
              <Button variant='outline'>Cancel</Button>
            </DialogClose>
            <DialogClose asChild>
              <Button onClick={parseAndSaveEnvFile}>Import</Button>
            </DialogClose>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}

export const EnvironmentVariablesDialog: React.FC<{
  showEnvDialog: boolean
  setShowEnvDialog: (show: boolean) => void
}> = ({ showEnvDialog, setShowEnvDialog }) => {
  return (
    <Dialog open={showEnvDialog} onOpenChange={setShowEnvDialog}>
      <DialogContent className='mt-12 max-h-[80vh] overflow-y-auto sm:max-w-none w-fit'>
        {/* DialogContent requires DialogTitle error  */}
        <DialogTitle className='sr-only'>Environment Variables</DialogTitle>
        <EnvironmentVariablesPanel />
      </DialogContent>
    </Dialog>
  )
}
