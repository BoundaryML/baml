import React from 'react'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '../components/ui/dialog'
import { atom, useAtom, useAtomValue } from 'jotai'
import { atomWithStorage, createJSONStorage } from 'jotai/utils'
import Form from '@rjsf/core'
import { FieldTemplateProps, IconButtonProps, ObjectFieldTemplateProps, RJSFSchema, UiSchema } from '@rjsf/utils'
import validator from '@rjsf/validator-ajv8'
import { Button } from '@/components/ui/button'
import { envvarStorageAtom } from '@/shared/Storage'
import { runtimeRequiredEnvVarsAtom } from '../baml_wasm_web/EventListener'
//import { Form, FormControl, FormDescription, FormField, FormItem, FormLabel, FormMessage } from '@/components/ui/form'
//Using ShadCN, create a form that allows submitting record types mapping string to string
import { Input } from '@/components/ui/input'
import { PlusIcon, Trash2Icon } from 'lucide-react'
import z from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'
import { useForm } from 'react-hook-form'

const showSettingsAtom = atom(true)

const envvarsAtom = atom(
  (get) => {
    const requiredEnvvars = get(runtimeRequiredEnvVarsAtom).map((envvar) => [envvar, ''])
    const storedEnvvars = Object.entries(get(envvarStorageAtom))

    const allEnvvars = Object.fromEntries(requiredEnvvars.concat(storedEnvvars))

    return Object.entries(allEnvvars).map(([key, value]) => ({ key, value }))
  },
  (get, set, envvarsFormData: { key: string; value: string }[]) => {
    set(envvarStorageAtom, Object.fromEntries(envvarsFormData.map(({ key, value }) => [key, value])))
  },
)

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
    key: { 'ui:FieldTemplate': EnvvarFieldTemplate },
    value: { 'ui:FieldTemplate': EnvvarFieldTemplate },
  },
  'ui:submitButtonOptions': {
    norender: true,
  },
}

function EnvvarFieldTemplate(props: FieldTemplateProps) {
  const { children } = props
  return <div className='font-mono'>{children}</div>
}

const EnvVarFieldTemplate = (props: ObjectFieldTemplateProps) => {
  const requiredEnvvars = useAtomValue(runtimeRequiredEnvVarsAtom)
  const renderedProps = []

  for (const { name, content } of props.properties) {
    if (name === 'key' && requiredEnvvars.includes(content.props.formData)) {
      renderedProps.push(<p>(required)</p>)
    }
    renderedProps.push(content)
    if (name === 'key') {
      renderedProps.push(<p>=</p>)
    }
  }
  return <div className='flex flex-row'>{renderedProps}</div>
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

export const SettingsDialog: React.FC = () => {
  const [showSettings, setShowSettings] = useAtom(showSettingsAtom)
  const duplicate = false

  const [envvars, setEnvvars] = useAtom(envvarsAtom)

  return (
    <Dialog open={showSettings} onOpenChange={setShowSettings}>
      <DialogContent className='overflow-y-scroll max-h-screen bg-vscode-editorWidget-background border-vscode-textSeparator-foreground overflow-x-clip'>
        <DialogHeader className='flex flex-row gap-x-4 items-center'>
          <DialogTitle className='text-s font-semibold'>Environment variables</DialogTitle>
        </DialogHeader>
        <Form
          schema={schema}
          uiSchema={uiSchema}
          validator={validator}
          formData={envvars}
          onChange={(d) => setEnvvars(d.formData)}
          templates={{
            ObjectFieldTemplate: EnvVarFieldTemplate,
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

export { showSettingsAtom }
