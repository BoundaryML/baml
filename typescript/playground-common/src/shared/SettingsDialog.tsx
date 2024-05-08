import React from 'react'
import { Button } from '@/components/ui/button'
import { Input, InputProps } from '../components/ui/input'
import Form from '@rjsf/core'
import type {
  ArrayFieldTemplateItemType,
  FieldTemplateProps,
  IconButtonProps,
  ObjectFieldTemplateProps,
  RJSFSchema,
  UiSchema,
} from '@rjsf/utils'
import validator from '@rjsf/validator-ajv8'
import { atom, useAtom, useSetAtom, useAtomValue } from 'jotai'
import { atomWithStorage, createJSONStorage } from 'jotai/utils'
import { EyeOffIcon as HideIcon, EyeIcon as ShowIcon, PlusIcon, SettingsIcon, Trash2Icon } from 'lucide-react'
import { envvarStorageAtom, runtimeRequiredEnvVarsAtom } from '../baml_wasm_web/EventListener'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '../components/ui/dialog'
import clsx from 'clsx'

export const showSettingsAtom = atom(true)
export const showEnvvarValuesAtom = atom(false)

const envvarsAtom = atom(
  (get) => {
    const requiredEnvvars = get(runtimeRequiredEnvVarsAtom).map((envvar) => [envvar, ''])
    const storedEnvvars = Object.entries(get(envvarStorageAtom))

    const allEnvvars = Object.fromEntries(requiredEnvvars.concat(storedEnvvars))

    return Object.entries(allEnvvars).map(([key, value]) => ({ key, value }))
  },
  (get, set, envvarsFormData: { key?: string; value?: string }[]) => {
    set(
      envvarStorageAtom,
      Object.fromEntries(
        envvarsFormData
          .filter(({ key }) => typeof key === 'string' && key.length)
          .map(({ key, value }) => [key, value]),
      ),
    )
  },
)

// const EnvvarKeyInput: React.FC<InputProps> = ({ className, type, ...props }) => {
//   const requiredEnvvars = useAtomValue(runtimeRequiredEnvVarsAtom)
//   if (requiredEnvvars.includes(props.formData)) {
//     return (
//       <div className='bg-grey-500 font-mono outline-none focus:outline focus:outline-1 focus:outline-white'>
//         {props.formData.value}
//       </div>
//     )
//   }
//   return (
//     <Input
//       {...props}
//       className='bg-grey-500 font-mono outline-none focus:outline focus:outline-1 focus:outline-white'
//       autoComplete='none'
//     />
//   )
// }

const EnvvarValueInput: React.FC<InputProps> = ({ className, type, ...props }) => {
  const showEnvvarValues = useAtomValue(showEnvvarValuesAtom)
  return (
    <Input
      {...props}
      className='bg-grey-500 font-mono outline-none focus:outline focus:outline-1 focus:outline-white'
      autoComplete='off'
      type={showEnvvarValues ? 'text' : 'password'}
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
      //'ui:FieldTemplate': EnvvarFieldTemplate,
      //'ui:widget': EnvvarKeyInput,
    },
    value: {
      //'ui:FieldTemplate': EnvvarFieldTemplate,
      //'ui:widget': EnvvarValueInput,
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
    <div key={key} className='flex flex-row items-center'>
      {children}
      <div className='grow'>
        {fieldItemIsRequired ? (
          <p className='justify-self-end text-xs'>(required)</p>
        ) : (
          <Button
            size={'icon'}
            className='!flex flex-col !px-0 !py-0 hover:bg-red-700 h-fit !max-w-[48px] ml-auto'
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
  const { children } = props
  return children
}

const EnvvarEntryTemplate = (props: ObjectFieldTemplateProps) => {
  const requiredEnvvars = useAtomValue(runtimeRequiredEnvVarsAtom)
  const renderedProps = []

  for (const { name, content } of props.properties) {
    renderedProps.push(content)
    if (name === 'key') {
      renderedProps.push(<p className='h-4'>=</p>)
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

//ObjectFieldTemplate: EnvvarEntryTemplate,
//ArrayFieldItemTemplate,
export default SettingsDialog
