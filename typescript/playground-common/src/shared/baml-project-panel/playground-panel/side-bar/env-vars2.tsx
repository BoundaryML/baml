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
    <div className='container mx-auto py-8 max-w-4xl'>
      <div className='flex justify-between items-center mb-6'>
        <h1 className='text-2xl font-bold'>Environment Variables</h1>
        <div className='flex gap-2'>
          <Dialog>
            <DialogTrigger asChild>
              <Button variant='outline' className='flex items-center gap-2'>
                <FileText className='h-4 w-4' />
                Import .env
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

          <Button onClick={saveEnvVars} disabled={!hasUnsavedChanges} className='flex items-center gap-2'>
            <Save className='h-4 w-4' />
            Save Changes
          </Button>
        </div>
      </div>

      {hasUnsavedChanges && (
        <div className='bg-amber-50 border border-amber-200 rounded-md p-3 mb-6 flex items-center gap-2'>
          <AlertTriangle className='h-5 w-5 text-amber-500' />
          <p className='text-amber-800'>You have unsaved changes. Click "Save Changes" to persist them.</p>
        </div>
      )}

      <div className='bg-white rounded-md border shadow-sm'>
        <div className='grid grid-cols-[1fr_1fr_auto] gap-4 p-4 border-b bg-gray-50 font-medium'>
          <div>Key</div>
          <div>Value</div>
          <div className='w-24 text-center'>Actions</div>
        </div>

        {envVars.map((env, index) => (
          <div key={index} className='grid grid-cols-[1fr_1fr_auto] gap-4 p-4 border-b items-center'>
            <div className='flex items-center gap-2'>
              <Input value={env.key} readOnly className='bg-gray-50 cursor-not-allowed' />
              {REQUIRED_ENV_VARS.includes(env.key) && env.value === '' && (
                <Badge variant='outline' className='text-red-500 border-red-200 bg-red-50'>
                  &lt;unset&gt;
                </Badge>
              )}
            </div>
            <div className='flex items-center gap-2'>
              <Input
                type={env.hidden ? 'password' : 'text'}
                value={env.value}
                onChange={(e) => updateEnvVar(index, e.target.value)}
                placeholder={REQUIRED_ENV_VARS.includes(env.key) && env.value === '' ? '<unset>' : ''}
                className={REQUIRED_ENV_VARS.includes(env.key) && env.value === '' ? 'border-red-200' : ''}
              />
            </div>
            <div className='flex items-center justify-end gap-2'>
              <Button
                variant='ghost'
                size='icon'
                onClick={() => toggleVisibility(index)}
                title={env.hidden ? 'Show value' : 'Hide value'}
              >
                {env.hidden ? <Eye className='h-4 w-4' /> : <EyeOff className='h-4 w-4' />}
              </Button>
              <AlertDialog>
                <AlertDialogTrigger asChild>
                  <Button variant='ghost' size='icon' className='text-red-500 hover:text-red-600' title='Delete'>
                    <Trash2 className='h-4 w-4' />
                  </Button>
                </AlertDialogTrigger>
                <AlertDialogContent>
                  <AlertDialogHeader>
                    <AlertDialogTitle>Delete Environment Variable</AlertDialogTitle>
                    <AlertDialogDescription>
                      Are you sure you want to delete "{env.key}"? This action cannot be undone.
                    </AlertDialogDescription>
                  </AlertDialogHeader>
                  <AlertDialogFooter>
                    <AlertDialogCancel>Cancel</AlertDialogCancel>
                    <AlertDialogAction onClick={() => deleteEnvVar(index)} className='bg-red-500 hover:bg-red-600'>
                      Delete
                    </AlertDialogAction>
                  </AlertDialogFooter>
                </AlertDialogContent>
              </AlertDialog>
            </div>
          </div>
        ))}

        <div className='grid grid-cols-[1fr_1fr_auto] gap-4 p-4'>
          <Input placeholder='Add new key' value={newKey} onChange={(e) => setNewKey(e.target.value)} />
          <Input placeholder='Add new value' value={newValue} onChange={(e) => setNewValue(e.target.value)} />
          <Button onClick={addEnvVar} disabled={!newKey.trim()} className='flex items-center gap-2'>
            <Plus className='h-4 w-4' />
            Add
          </Button>
        </div>
      </div>
    </div>
  )
}
