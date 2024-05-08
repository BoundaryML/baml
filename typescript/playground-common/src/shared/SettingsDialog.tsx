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
import { envvarStorageAtom, runtimeRequiredEnvVarsAtom } from '../baml_wasm_web/EventListener'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '../components/ui/dialog'
import clsx from 'clsx'

export const showSettingsAtom = atom(true)
export const showEnvvarValuesAtom = atom(false)

const envvarsAtom = atom(
  (get) => {
    const storedEnvvars = get(envvarStorageAtom)
    const requiredButUnset = get(runtimeRequiredEnvVarsAtom)
      .filter((k) => !storedEnvvars.some(({ key }) => k === key))
      .map((key) => ({ key, value: '' }))

    return requiredButUnset.concat(storedEnvvars)
  },
  (get, set, envvarsFormData: { key: string; value: string }[]) => {
    set(envvarStorageAtom, envvarsFormData)
  },
)

const EnvvarKeyInput: React.FC<WidgetProps> = (props) => {
  const requiredEnvvars = useAtomValue(runtimeRequiredEnvVarsAtom)
  return (
    <Input
      id={props.id}
      name={props.name}
      type='text'
      className='bg-grey-500 outline-none focus:outline focus:outline-1 focus:outline-white'
      value={props.value}
      disabled={requiredEnvvars.includes(props.value)}
      onChange={(event) => props.onChange(event.target.value)}
    />
  )
}

const EnvvarValueInput: React.FC<WidgetProps> = (props) => {
  const showEnvvarValues = useAtomValue(showEnvvarValuesAtom)
  return (
    <Input
      id={props.id}
      name={props.name}
      type={showEnvvarValues ? 'text' : 'password'}
      placeholder='(unset)'
      className='bg-grey-500 group-[.required-env-var-not-set]:outline group-[.required-env-var-not-set]:outline-yellow-500 focus:outline focus:outline-3 focus:outline-white'
      value={props.value}
      onChange={(event) => props.onChange(event.target.value)}
    />
  )
}

const schema: RJSFSchema = {
  type: 'array',
  items: {
    type: 'object',
    properties: {
      key: {
        type: 'string',
        title: 'Key',
      },
      value: {
        type: 'string',
        title: 'Value',
      },
    },
  },
}

const uiSchema: UiSchema = {
  items: {
    'ui:classNames': 'flex flex-row',
    key: {
      'ui:FieldTemplate': EnvvarFieldTemplate,
      'ui:widget': EnvvarKeyInput,
    },
    value: {
      'ui:FieldTemplate': EnvvarFieldTemplate,
      'ui:widget': EnvvarValueInput,
    },
  },
  'ui:options': {
    orderable: false,
  },
  'ui:submitButtonOptions': {
    norender: true,
  },
}

function ArrayFieldItemTemplate(props: ArrayFieldTemplateItemType) {
  const requiredEnvvars = useAtomValue(runtimeRequiredEnvVarsAtom)
  const { key, children, className, index, onDropIndexClick } = props
  const fieldItemIsRequired = requiredEnvvars.includes(children.props.formData.key)
  return (
    <div key={key} className='flex flex-row items-center gap-2 border-none pb-2'>
      {children}
      <div className='grow'>
        {fieldItemIsRequired ? (
          <p className='justify-self-end text-xs min-w-24'>(used in BAML)</p>
        ) : (
          <Button
            size={'icon'}
            className='!flex flex-col px-2 py-2 mr-2 text-color-white bg-transparent hover:bg-red-600 h-fit !max-w-[48px] ml-auto'
            onClick={onDropIndexClick(index)}
            disabled={fieldItemIsRequired}
          >
            <Trash2Icon size={14} />
          </Button>
        )}
      </div>
    </div>
  )
}

function EnvvarFieldTemplate(props: FieldTemplateProps) {
  return <div>{props.children}</div>
}

const EnvvarEntryTemplate = (props: ObjectFieldTemplateProps) => {
  const requiredEnvvars = useAtomValue(runtimeRequiredEnvVarsAtom)

  const renderedProps = []

  const classNames = []

  if (requiredEnvvars.includes(props.formData.key) && props.formData.value === '') {
    classNames.push('required-env-var-not-set')
  }

  for (const { name, content } of props.properties) {
    renderedProps.push(content)
    if (name === 'key') {
      renderedProps.push(<div className='h-9 py-1.5'>=</div>)
    }
  }
  return <div className={clsx('flex flex-row items-center gap-2 font-mono group', classNames)}>{renderedProps}</div>
}

function AddButton(props: IconButtonProps) {
  const { icon, iconType, ...btnProps } = props
  return (
    <Button
      variant='ghost'
      size='icon'
      {...btnProps}
      className='flex flex-row items-center p-1 text-xs w-fit h-fit gap-x-2 hover:bg-vscode-descriptionForeground'
    >
      <PlusIcon size={14} /> <div>Add item</div>
    </Button>
  )
}

function RemoveButton(props: IconButtonProps) {
  const { icon, iconType, ...btnProps } = props
  return (
    <div className='flex w-fit h-fit'>
      <Button
        {...btnProps}
        size={'icon'}
        className='!flex flex-col !px-0 !py-0 hover:bg-red-700 h-fit !max-w-[48px] ml-auto'
      >
        <Trash2Icon size={14} />
      </Button>
    </div>
  )
}

export const ShowSettingsButton: React.FC<{ buttonClassName: string; iconClassName: string }> = ({
  buttonClassName,
  iconClassName,
}) => {
  const setShowSettings = useSetAtom(showSettingsAtom)
  return (
    <Button className={buttonClassName} onClick={() => setShowSettings(true)}>
      <SettingsIcon className={iconClassName} />
    </Button>
  )
}
export const SettingsDialog: React.FC = () => {
  const [showSettings, setShowSettings] = useAtom(showSettingsAtom)
  const [showEnvvarValues, setShowEnvvarValues] = useAtom(showEnvvarValuesAtom)

  const [envvars, setEnvvars] = useAtom(envvarsAtom)

  return (
    <Dialog open={showSettings} onOpenChange={setShowSettings}>
      <DialogContent className='overflow-y-scroll max-h-screen bg-vscode-editorWidget-background border-vscode-textSeparator-foreground overflow-x-clip'>
        <DialogHeader className='flex flex-row gap-x-4 items-end'>
          <DialogTitle className='text-s font-semibold'>Environment variables</DialogTitle>
          <Button
            variant='ghost'
            size='icon'
            className='flex flex-row items-center p-1 text-xs w-fit h-fit gap-x-2 hover:bg-vscode-descriptionForeground'
            onClick={() => setShowEnvvarValues((prev) => !prev)}
          >
            {showEnvvarValues ? <ShowIcon className='h-4' /> : <HideIcon className='h-4' />}
          </Button>
        </DialogHeader>
        <Form
          autoComplete='off'
          schema={schema}
          uiSchema={uiSchema}
          validator={validator}
          formData={envvars}
          onChange={(d) => setEnvvars(d.formData)}
          templates={{
            ObjectFieldTemplate: EnvvarEntryTemplate,
            ArrayFieldItemTemplate,
            ButtonTemplates: {
              AddButton,
              RemoveButton,
            },
          }}
        />
      </DialogContent>
    </Dialog>
  )
}

export default SettingsDialog
