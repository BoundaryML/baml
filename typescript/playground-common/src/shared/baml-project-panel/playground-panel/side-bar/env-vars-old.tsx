'use client'

import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import { useToast } from '@/components/hooks/use-toast'
import { motion } from 'motion/react'
import { atom, useAtomValue, useSetAtom } from 'jotai'
import {
  AlertTriangle,
  Check,
  ChevronRight,
  Circle,
  CircleDot,
  Edit2,
  Eye,
  EyeOff,
  Info,
  PlusCircle,
  Settings2,
  XCircle,
} from 'lucide-react'
import { useState } from 'react'
import { envVarsAtom, proxyUrlAtom, requiredEnvVarsAtom, userEnvVarsAtom } from '../../atoms'
import { bamlConfig, type BamlConfigAtom } from '../../../../baml_wasm_web/bamlConfig'
import { cn } from '@/lib/utils'
import { Switch } from '@radix-ui/react-switch'
import { QuestionMarkCircledIcon, QuestionMarkIcon } from '@radix-ui/react-icons'
import { Checkbox } from '@/components/ui/checkbox'
import { vscode } from '../../vscode'

const renderedEnvVarsAtom = atom((get) => {
  const userEnvVars = get(userEnvVarsAtom)
  const requiredEnvVars = get(requiredEnvVarsAtom)

  const vars = Object.entries(userEnvVars).map(([key, value]) => ({
    key,
    value,
    required: requiredEnvVars.includes(key),
  }))

  const missingVars = requiredEnvVars.filter((envVar) => !(envVar in userEnvVars))

  vars.push(
    ...missingVars.map((envVar) => ({
      key: envVar,
      value: undefined,
      required: true,
    })),
  )

  vars.sort((a, b) => {
    if (a.required !== b.required) {
      return b.required ? 1 : -1 // Required vars first
    }
    return a.key.localeCompare(b.key) // Then alphabetically within each group
  })
  return vars
})

export default function EnvVars() {
  const envVars = useAtomValue(renderedEnvVarsAtom)
  const setUserEnvVars = useSetAtom(userEnvVarsAtom)
  const currentUserEnvVars = useAtomValue(userEnvVarsAtom)
  const proxySettings = useAtomValue(proxyUrlAtom)
  const setBamlConfig = useSetAtom(bamlConfig)

  const [editingKey, setEditingKey] = useState<string | null>(null)
  const [editValue, setEditValue] = useState('')
  const [newKey, setNewKey] = useState('')
  const [showPassword, setShowPassword] = useState(false)
  const { toast } = useToast()

  const handleEdit = (key: string, value: string | undefined) => {
    setEditingKey(key)
    setEditValue(value ?? '')
    setShowPassword(false)
  }

  const handleSave = async (key: string) => {
    try {
      await new Promise((resolve) => setTimeout(resolve, 0))
      setUserEnvVars({ ...currentUserEnvVars, [key]: editValue })
      setEditingKey(null)
      toast({
        title: 'Environment variable updated',
        description: `${key} has been successfully updated.`,
      })
    } catch (error) {
      toast({
        title: 'Error',
        description: 'Failed to update environment variable.',
        variant: 'destructive',
      })
    }
  }

  const handleAddNew = () => {
    if (newKey.trim() === '') {
      toast({
        title: 'Error',
        description: 'Variable name cannot be empty.',
        variant: 'destructive',
      })
      return
    }
    if (envVars.some((v) => v.key === newKey)) {
      toast({
        title: 'Error',
        description: 'Variable already exists.',
        variant: 'destructive',
      })
      return
    }
    setUserEnvVars({ ...currentUserEnvVars, [newKey]: '' })
    setNewKey('')
    toast({
      title: 'New variable added',
      description: `${newKey} has been added to the environment variables.`,
    })
  }

  return (
    <>
      <div className='p-2 space-y-2 text-xs'>
        <h3 className='flex gap-2 items-center font-medium text-muted-foreground'>
          <Settings2 className='w-4 h-4' />
          Environment Variables
        </h3>
        <div className='text-left text-muted-foreground'>
          <p>Set your own API Keys here.</p>
          <a
            href='https://docs.boundaryml.com/ref/llm-client-providers/openai-generic'
            target='_blank'
            rel='noopener noreferrer'
            className='text-blue-500 hover:underline'
          >
            See supported LLMs
          </a>
        </div>
        <div className='text-left text-muted-foreground'>
          <div className='flex gap-2 items-center'>
            <p className='flex gap-2 items-center'>
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
                    To get around this, the BAML VSCode extension includes a <b>localhost proxy</b> that sits between
                    your browser and the LLM provider's API.
                    <br />
                    <br />
                    <b>BAML MAKES NO NETWORK CALLS BEYOND THE LLM PROVIDER'S API YOU SPECIFY.</b>
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
              VSCode proxy is <b>{proxySettings.proxyEnabled ? 'enabled' : 'disabled'}</b>
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
            </p>
            <p>{proxySettings.proxyUrl}</p>
          </div>
        </div>
        <div className='space-y-1'>
          {envVars
            .filter(({ key }) => key !== 'BOUNDARY_PROXY_URL')
            .map(({ key, value, required }, index) => (
              <TooltipProvider key={key} delayDuration={300}>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <motion.div
                      initial={{ opacity: 0, y: 5 }}
                      animate={{ opacity: 1, y: 0 }}
                      transition={{ delay: index * 0.02 }}
                      className='group relative flex items-center gap-2 rounded-sm px-1 py-0.5 transition-colors hover:bg-muted/30'
                    >
                      <div className='flex relative gap-2 items-center w-fit'>
                        <div className='flex gap-2 items-center group-hover:invisible'>
                          {required ? (
                            <CircleDot className='w-3 h-3 text-muted-foreground' />
                          ) : (
                            <Circle className='w-3 h-3 text-muted-foreground' />
                          )}
                          {!value || value === '' ? (
                            <AlertTriangle className='h-4 w-4 rounded-full bg-orange-400 p-0.5 text-white' />
                          ) : (
                            <Check className='h-4 w-4 rounded-full bg-green-500 p-0.5 text-white' />
                          )}
                        </div>

                        <div className='hidden absolute left-0 gap-2 items-center group-hover:flex'>
                          <Button
                            variant='ghost'
                            size='sm'
                            className='p-0 w-4 h-4'
                            onClick={(e) => {
                              e.preventDefault()
                              const newVars = { ...currentUserEnvVars }
                              delete newVars[key]
                              setUserEnvVars(newVars)
                            }}
                          >
                            <XCircle className='w-4 h-4 text-muted-foreground hover:text-destructive' />
                          </Button>
                          <Button
                            variant='ghost'
                            size='sm'
                            className='p-0 w-4 h-4'
                            onClick={(e) => {
                              e.preventDefault()
                              handleEdit(key, value as string)
                            }}
                          >
                            <Edit2 className='w-4 h-4 text-muted-foreground hover:text-primary' />
                          </Button>
                        </div>
                      </div>

                      <code className='font-mono text-xs transition-colors text-muted-foreground group-hover:text-foreground'>
                        {key}
                      </code>
                    </motion.div>
                  </TooltipTrigger>
                  <TooltipContent side='top' className='text-xs'>
                    {value !== undefined && value !== '' ? 'Click to edit' : 'Variable needs to be set'}
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
            ))}
        </div>
        <div className='flex items-center mt-4 space-x-2'>
          <Input
            value={newKey}
            onChange={(e) => setNewKey(e.target.value)}
            placeholder='Var name'
            className='h-8 text-xs'
          />
          <Button size='sm' variant={'outline'} onClick={handleAddNew}>
            <PlusCircle className='mr-2 w-4 h-4' />
            Add
          </Button>
        </div>
      </div>

      <Dialog open={editingKey !== null} onOpenChange={(open) => !open && setEditingKey(null)}>
        <DialogContent className='sm:max-w-[425px]'>
          <DialogHeader>
            <DialogTitle className='text-sm'>Edit Environment Variable: {editingKey}</DialogTitle>
          </DialogHeader>
          <div className='relative'>
            <Input
              type={showPassword ? 'text' : 'password'}
              value={editValue}
              onChange={(e) => setEditValue(e.target.value)}
              className='pr-8'
            />
            <Button
              type='button'
              variant='ghost'
              size='sm'
              className='absolute top-0 right-0 px-3 h-full'
              onClick={() => setShowPassword(!showPassword)}
            >
              {showPassword ? <EyeOff className='w-4 h-4' /> : <Eye className='w-4 h-4' />}
            </Button>
          </div>
          <DialogFooter>
            <Button type='button' variant='ghost' onClick={() => setEditingKey(null)}>
              Cancel
            </Button>
            <Button
              type='submit'
              onClick={() => {
                if (editingKey) {
                  void handleSave(editingKey)
                }
              }}
            >
              Save
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
