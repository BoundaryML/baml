import React, { type ChangeEvent, type FocusEvent, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input, InputProps } from '../components/ui/input'
import Form from '@rjsf/core'
import {
  ArrayFieldTemplateItemType,
  BaseInputTemplateProps,
  FieldTemplateProps,
  IconButtonProps,
  ObjectFieldTemplateProps,
  RJSFSchema,
  UiSchema,
  WidgetProps,
  ariaDescribedByIds,
  examplesId,
  getInputProps,
  titleId,
} from '@rjsf/utils'
import validator from '@rjsf/validator-ajv8'
import { atom, useAtom, useSetAtom, useAtomValue } from 'jotai'
import { atomWithStorage, createJSONStorage } from 'jotai/utils'
import {
  EyeOffIcon as HideIcon,
  EyeIcon as ShowIcon,
  EqualIcon,
  PlusIcon,
  SettingsIcon,
  Trash2Icon,
} from 'lucide-react'
import { envKeyValuesAtom, runtimeRequiredEnvVarsAtom } from '../baml_wasm_web/EventListener'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '../components/ui/dialog'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../components/ui/tooltip'
import clsx from 'clsx'
import { Checkbox } from '../components/ui/checkbox'

export const showSettingsAtom = atom(false)
const showEnvvarValuesAtom = atom(false)

const tracingEnvVarsAtom = atom(['BOUNDARY_PROJECT_ID', 'BOUNDARY_SECRET'])
const configEnvVarsAtom = atom(() => {
  if ((window as any).next) {
    return ['BOUNDARY_ANTHROPIC_PROXY_URL']
  } else {
    return ['BOUNDARY_ANTHROPIC_PROXY_URL']
  }
})

const envvarsAtom = atom((get) => {
  const storedEnvvars = get(envKeyValuesAtom)
  const requiredVarNames = get(runtimeRequiredEnvVarsAtom)
  const tracingVarNames = get(tracingEnvVarsAtom)
  const configVarNames = get(configEnvVarsAtom)

  // Create a copy of requiredVarNames and tracingVarNames to manipulate
  let requiredVarNamesCopy = [...requiredVarNames]
  let tracingVarNamesCopy = [...tracingVarNames]
  let configVarNamesCopy = [...configVarNames]

  // Update the stored envvars type for the runtime.
  const envVars = storedEnvvars.map(([key, value, index]): EnvVar => {
    if (requiredVarNamesCopy.includes(key)) {
      // Remove the key from the copy of requiredVarNames
      requiredVarNamesCopy = requiredVarNamesCopy.filter((varName) => varName !== key)
      return { key, value, type: 'baml', index }
    }
    if (tracingVarNamesCopy.includes(key)) {
      // Remove the key from the copy of tracingVarNames
      tracingVarNamesCopy = tracingVarNamesCopy.filter((varName) => varName !== key)
      return { key, value, type: 'tracing', index }
    }
    if (configVarNamesCopy.includes(key)) {
      // Remove the key from the copy of configVarNames
      configVarNamesCopy = configVarNamesCopy.filter((varName) => varName !== key)
      return { key, value, type: 'config', index }
    }
    return { key, value, type: 'user', index }
  })

  // Add required but unset envvars
  const requiredButUnset = requiredVarNamesCopy
    .filter((k) => !envVars.some(({ key }) => k === key))
    .map((k): EnvVar => ({ key: k, value: '', type: 'baml', index: null }))
  const tracingEnvVars = tracingVarNamesCopy
    .filter((k) => !envVars.some(({ key }) => k === key))
    .map((k): EnvVar => ({ key: k, value: '', type: 'tracing', index: null }))
  const configEnvVars = configVarNamesCopy
    .filter((k) => !envVars.some(({ key }) => k === key))
    .map((k): EnvVar => ({ key: k, value: '', type: 'config', index: null }))

  // Sort by type (baml, tracing) are sorted by name, user is sorted by index

  const keys = [...envVars, ...requiredButUnset, ...tracingEnvVars, ...configEnvVars].sort((a, b) => {
    if (a.type === 'user' && b.type === 'user') {
      return a.index! - b.index!
    }
    if (a.type === 'user') {
      return 1
    }
    if (b.type === 'user') {
      return -1
    }
    return a.key.localeCompare(b.key)
  })

  return keys
})

const requiredButUnsetAtom = atom((get) => {
  const envvars = get(envvarsAtom)
  return get(runtimeRequiredEnvVarsAtom).filter(
    (k) => !envvars.some(({ key, value }) => k === key && value && value.length > 0),
  )
})

type EnvVar = { key: string; value: string; type: 'baml' | 'tracing' | 'user' | 'config'; index: number | null }
const EnvvarInput: React.FC<{ envvar: EnvVar }> = ({ envvar }) => {
  const [showEnvvarValues] = useAtom(showEnvvarValuesAtom)
  const setEnvKeyValue = useSetAtom(envKeyValuesAtom)
  return (
    <div className='flex flex-row items-center gap-2 my-2'>
      <Input
        type='text'
        value={envvar.key}
        disabled={envvar.type !== 'user'}
        onChange={(e) => {
          if (envvar.index !== null) {
            setEnvKeyValue({
              itemIndex: envvar.index,
              newKey: e.target.value,
            })
          } else {
            setEnvKeyValue({ itemIndex: null, key: e.target.value })
          }
        }}
        className='font-mono outline-none bg-vscode-input-background focus-visible:outline focus:outline-2 focus:outline-vscode-input-border'
      />
      <span>=</span>
      <Input
        type={showEnvvarValues ? 'text' : 'password'}
        value={envvar.value}
        placeholder='(unset)'
        onChange={(e) => {
          if (envvar.index !== null) {
            setEnvKeyValue({
              itemIndex: envvar.index,
              value: e.target.value,
            })
          } else {
            setEnvKeyValue({ itemIndex: null, key: envvar.key, value: e.target.value })
          }
        }}
        className={`bg-vscode-input-background outline-none focus-visible:outline focus:outline-0 focus:outline-white font-mono ${
          !envvar.value && envvar.type === 'baml' ? 'outline outline-2 outline-yellow-500' : ''
        }`}
      />
      {envvar.type === 'user' && envvar.index !== null && (
        <Button
          size={'icon'}
          className='!flex flex-col px-2 py-2 text-color-white bg-transparent hover:bg-red-600 h-fit !max-w-[48px] ml-auto'
          onClick={() => {
            if (envvar.index !== null) {
              setEnvKeyValue({ itemIndex: envvar.index, remove: true })
            }
          }}
        >
          <Trash2Icon size={14} />
        </Button>
      )}
    </div>
  )
}

export const ShowSettingsButton: React.FC<{ buttonClassName: string; iconClassName: string }> = ({
  buttonClassName,
  iconClassName,
}) => {
  const setShowSettings = useSetAtom(showSettingsAtom)
  const requiredButUnset = useAtomValue(requiredButUnsetAtom)
  const requiredButUnsetCount = requiredButUnset.length

  const button = (
    <Button className={buttonClassName} onClick={() => setShowSettings(true)}>
      <SettingsIcon className={iconClassName} />
      {requiredButUnsetCount > 0 && (
        <div className='absolute inline-flex items-center justify-center w-6 h-6 text-xs font-bold text-white bg-yellow-500 border-2 border-white rounded-full -top-0 -end-0 dark:border-gray-900'>
          {requiredButUnsetCount}
        </div>
      )}
    </Button>
  )
  if (requiredButUnsetCount === 0) {
    return button
  }

  const message =
    requiredButUnsetCount === 1
      ? `env.${requiredButUnset[0]} is used but not set`
      : requiredButUnsetCount === 2
        ? `${requiredButUnset.map((k) => `env.${k}`).join(' and ')} are used but not set`
        : `${requiredButUnsetCount} environment variables are used but not set`
  return (
    <TooltipProvider>
      <Tooltip delayDuration={100}>
        <TooltipTrigger asChild>{button}</TooltipTrigger>
        <TooltipContent className='flex flex-col gap-y-1'>{message}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  )
}
export const SettingsDialog: React.FC = () => {
  const [showSettings, setShowSettings] = useAtom(showSettingsAtom)
  const [showEnvvarValues, setShowEnvvarValues] = useAtom(showEnvvarValuesAtom)
  const envvars = useAtomValue(envvarsAtom)
  const setEnvKeyValue = useSetAtom(envKeyValuesAtom)
  const [enableObservability, setEnableObservability] = useState(
    envvars.some((t) => t.type === 'tracing' && t.value.length > 0),
  )

  return (
    <Dialog open={showSettings} onOpenChange={setShowSettings}>
      <DialogContent className=' min-h-[550px] max-h-[550px] overflow-y-auto bg-vscode-editorWidget-background flex flex-col border-vscode-textSeparator-foreground overflow-x-clip'>
        <DialogHeader className='flex flex-row items-end gap-x-4'>
          <DialogTitle className='font-semibold'>Environment variables</DialogTitle>
          <Button
            variant='ghost'
            size='icon'
            className='flex flex-row items-center p-1 py-0 text-xs w-fit h-fit gap-x-2 hover:bg-vscode-descriptionForeground'
            onClick={() => setShowEnvvarValues((prev) => !prev)}
          >
            {showEnvvarValues ? <ShowIcon className='h-4' /> : <HideIcon className='h-4' />}
          </Button>
        </DialogHeader>
        <div className='flex flex-col gap-2 gap-y-6'>
          <div className='flex flex-col gap-1'>
            <div className='flex flex-row gap-2'>
              <span className='text-sm text-vscode-foreground'>
                Observability {enableObservability ? 'Enabled' : 'Disabled'}
              </span>
              <Checkbox
                className='border'
                checked={enableObservability}
                onCheckedChange={(e) => setEnableObservability((p) => !p)}
              />
            </div>
            {enableObservability && (
              <>
                <p className='text-xs italic text-vscode-descriptionForeground'>
                  You can get these from{' '}
                  <a className='text-blue-400 underline' target='_blank' href='https://app.boundaryml.com'>
                    Boundary Studio
                  </a>
                </p>
                {envvars
                  .filter((t) => t.type === 'tracing')
                  .map((envvar) => (
                    <EnvvarInput key={envvar.key} envvar={envvar} />
                  ))}
              </>
            )}
          </div>
          <div className='flex flex-col gap-1'>
            <span className='text-sm text-vscode-foreground'>From .baml files</span>
            <p className='text-xs italic text-vscode-descriptionForeground'>
              Environment variables are loaded lazily, only set any you want to use.
            </p>
            {envvars
              .filter((t) => t.type === 'baml')
              .map((envvar) => (
                <EnvvarInput key={envvar.key} envvar={envvar} />
              ))}
          </div>
          <div className='flex flex-col gap-1'>
            <span className='text-sm text-vscode-foreground'>Extra environment variables</span>
            {envvars
              .filter((t) => t.type === 'user')
              .map((envvar) => (
                <EnvvarInput key={envvar.index} envvar={envvar} />
              ))}
            <Button
              variant='ghost'
              size='icon'
              className='flex flex-row items-center p-1 text-xs w-fit h-fit gap-x-2 hover:bg-vscode-descriptionForeground'
              onClick={() => setEnvKeyValue({ itemIndex: null, key: 'NEW_ENV_VAR' })}
            >
              <PlusIcon size={14} /> <div>Add item</div>
            </Button>
          </div>

          {envvars.some((t) => t.type === 'config') && (
            <div className='flex flex-col gap-1'>
              <span className='text-sm text-vscode-foreground'>Internal vars</span>
              <span className='text-xs text-vscode-descriptionForeground'>
                Anthropic doesn't support client-side web calls, so we proxy the calls.
              </span>
              {envvars
                .filter((t) => t.type === 'config')
                .map((envvar) => (
                  <EnvvarInput key={envvar.index} envvar={envvar} />
                ))}
            </div>
          )}
        </div>
        <DialogFooter className='mt-auto'>
          <Button
            className='bg-vscode-button-hoverBackground text-vscode-button-foreground hover:bg-vscode-button-hoverBackground'
            onClick={() => setShowSettings(false)}
          >
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

export default SettingsDialog
