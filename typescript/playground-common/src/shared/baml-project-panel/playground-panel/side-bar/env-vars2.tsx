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
import { envVarsAtom, proxyUrlAtom, requiredEnvVarsAtom } from '../../atoms'
import { cn } from '@/lib/utils'
import { Switch } from '@radix-ui/react-switch'
import { QuestionMarkCircledIcon, QuestionMarkIcon } from '@radix-ui/react-icons'
import { Checkbox } from '@/components/ui/checkbox'
import { vscode } from '../../vscode'

import { useEffect, useRef } from 'react'
import { Textarea } from '@/components/ui/textarea'
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/components/ui/alert-dialog'
import { Trash2, Plus, Save, FileText } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Label } from '@/components/ui/label'

const renderedEnvVarsAtom = atom((get) => {
  const envVars = get(envVarsAtom)
  const requiredEnvVars = get(requiredEnvVarsAtom)

  const vars = Object.entries(envVars).map(([key, value]) => ({
    key,
    value,
    required: requiredEnvVars.includes(key),
  }))

  const missingVars = requiredEnvVars.filter((envVar) => !(envVar in envVars))

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

// Required environment variables that must be set
const REQUIRED_ENV_VARS = ['STRIPE_API_KEY', 'GOOGLE_API_KEY', 'AWS_API_KEY']

interface EnvVar {
  key: string
  value: string
  hidden: boolean
}

export default function EnvVariablesManager() {
  const [envVars, setEnvVars] = useState<EnvVar[]>([])
  const [hasUnsavedChanges, setHasUnsavedChanges] = useState(false)
  const [newKey, setNewKey] = useState('')
  const [newValue, setNewValue] = useState('')
  const [envFileContent, setEnvFileContent] = useState('')
  const [showPassword, setShowPassword] = useState(false)
  const { toast } = useToast()
  const initialLoadRef = useRef(true)

  // Load environment variables from localStorage on initial render
  useEffect(() => {
    const storedEnvVars = localStorage.getItem('envVars')
    if (storedEnvVars) {
      const parsedVars = JSON.parse(storedEnvVars)
      setEnvVars(parsedVars.sort((a, b) => a.key.localeCompare(b.key)))
    } else {
      // Initialize with required env vars as unset
      setEnvVars(
        REQUIRED_ENV_VARS.map((key) => ({
          key,
          value: '',
          hidden: true,
        })).sort((a, b) => a.key.localeCompare(b.key)),
      )
    }
    initialLoadRef.current = false
  }, [])

  // Track unsaved changes
  useEffect(() => {
    if (!initialLoadRef.current) {
      const storedEnvVars = localStorage.getItem('envVars')
      const currentEnvVarsString = JSON.stringify(envVars)

      if (storedEnvVars !== currentEnvVarsString) {
        setHasUnsavedChanges(true)
      } else {
        setHasUnsavedChanges(false)
      }
    }
  }, [envVars])

  // Save environment variables to localStorage
  const saveEnvVars = () => {
    localStorage.setItem('envVars', JSON.stringify(envVars))
    setHasUnsavedChanges(false)
  }

  // Toggle visibility of an environment variable
  const toggleVisibility = (index: number) => {
    const updatedEnvVars = [...envVars]
    updatedEnvVars[index].hidden = !updatedEnvVars[index].hidden
    setEnvVars(updatedEnvVars)
  }

  // Update an environment variable
  const updateEnvVar = (index: number, value: string) => {
    const updatedEnvVars = [...envVars]
    updatedEnvVars[index].value = value
    setEnvVars(updatedEnvVars)
  }

  // Delete an environment variable
  const deleteEnvVar = (index: number) => {
    const updatedEnvVars = [...envVars]
    updatedEnvVars.splice(index, 1)
    setEnvVars(updatedEnvVars)
  }

  // Add a new environment variable
  const addEnvVar = () => {
    if (newKey.trim() === '') return

    // Check if key already exists
    const keyExists = envVars.some((env) => env.key === newKey)
    if (keyExists) {
      // Update existing key
      const index = envVars.findIndex((env) => env.key === newKey)
      updateEnvVar(index, newKey, newValue)
    } else {
      // Add new key and sort
      const updatedEnvVars = [...envVars, { key: newKey, value: newValue, hidden: true }]
      setEnvVars(updatedEnvVars.sort((a, b) => a.key.localeCompare(b.key)))
    }

    // Reset form
    setNewKey('')
    setNewValue('')
  }

  // Parse and import environment variables from .env file
  const parseEnvFile = () => {
    const lines = envFileContent.split('\n')
    const parsedEnvVars: EnvVar[] = []

    // Keep existing env vars that aren't in the file
    const existingKeys = new Set()

    lines.forEach((line) => {
      // Skip comments and empty lines
      if (line.trim().startsWith('#') || line.trim() === '') return

      // Parse key-value pairs
      const match = line.match(/^([^=]+)=(.*)$/)
      if (match) {
        const key = match[1].trim()
        const value = match[2].trim()
        existingKeys.add(key)

        // Check if key already exists
        const existingIndex = envVars.findIndex((env) => env.key === key)
        if (existingIndex >= 0) {
          // Update existing env var
          parsedEnvVars.push({
            key,
            value,
            hidden: envVars[existingIndex].hidden,
          })
        } else {
          // Add new env var
          parsedEnvVars.push({
            key,
            value,
            hidden: true,
          })
        }
      }
    })

    // Add existing env vars that weren't in the file
    envVars.forEach((env) => {
      if (!existingKeys.has(env.key)) {
        parsedEnvVars.push(env)
      }
    })

    setEnvVars(parsedEnvVars.sort((a, b) => a.key.localeCompare(b.key)))
    setEnvFileContent('')
  }

  return (
    <div className='p-2 space-y-2 text-xs'>
      <h3 className='flex gap-2 items-center font-medium text-muted-foreground'>
        <Settings2 className='w-4 h-4' />
        Environment Variables
      </h3>

      {hasUnsavedChanges && (
        <div className='flex gap-2 items-center text-amber-500 bg-amber-50/50 p-2 rounded'>
          <AlertTriangle className='h-4 w-4' />
          <p className='text-amber-700'>You have unsaved changes</p>
          <Button size='sm' variant='ghost' onClick={saveEnvVars} className='ml-auto'>
            <Save className='h-4 w-4 mr-1' />
            Save Changes
          </Button>
        </div>
      )}

      <div className='space-y-1'>
        {envVars.map((env, index) => (
          <TooltipProvider key={env.key} delayDuration={300}>
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
                      {REQUIRED_ENV_VARS.includes(env.key) ? (
                        <CircleDot className='w-3 h-3 text-muted-foreground' />
                      ) : (
                        <Circle className='w-3 h-3 text-muted-foreground' />
                      )}
                      {!env.value || env.value === '' ? (
                        <AlertTriangle className='h-4 w-4 rounded-full bg-orange-400 p-0.5 text-white' />
                      ) : (
                        <Check className='h-4 w-4 rounded-full bg-green-500 p-0.5 text-white' />
                      )}
                    </div>

                    <div className='hidden absolute left-0 gap-2 items-center group-hover:flex'>
                      <Button variant='ghost' size='sm' className='p-0 w-4 h-4' onClick={() => deleteEnvVar(index)}>
                        <XCircle className='w-4 h-4 text-muted-foreground hover:text-destructive' />
                      </Button>
                      <Button variant='ghost' size='sm' className='p-0 w-4 h-4' onClick={() => toggleVisibility(index)}>
                        {env.hidden ? (
                          <Eye className='w-4 h-4 text-muted-foreground hover:text-primary' />
                        ) : (
                          <EyeOff className='w-4 h-4 text-muted-foreground hover:text-primary' />
                        )}
                      </Button>
                    </div>
                  </div>

                  <div className='flex-1 flex items-center gap-2'>
                    <code className='font-mono text-xs transition-colors text-muted-foreground group-hover:text-foreground'>
                      {env.key}
                    </code>
                    <Input
                      type={env.hidden ? 'password' : 'text'}
                      value={env.value}
                      onChange={(e) => updateEnvVar(index, e.target.value)}
                      className='h-6 text-xs'
                      placeholder={REQUIRED_ENV_VARS.includes(env.key) && !env.value ? 'Required' : undefined}
                    />
                  </div>
                </motion.div>
              </TooltipTrigger>
              <TooltipContent side='top' className='text-xs'>
                {env.value ? 'Click to edit' : 'Variable needs to be set'}
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
        ))}
      </div>

      <div className='flex items-center mt-4 space-x-2'>
        <Input
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
          placeholder='New variable name'
          className='h-8 text-xs'
        />
        <Button size='sm' variant='outline' onClick={addEnvVar} className='h-8'>
          <PlusCircle className='mr-2 w-4 h-4' />
          Add
        </Button>
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
              className='min-h-[200px] mt-2'
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
              <Button onClick={parseEnvFile}>Import</Button>
            </DialogClose>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
