import React from 'react'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '../components/ui/dialog'
import { atom, useAtom } from 'jotai'
import { atomWithStorage, createJSONStorage } from 'jotai/utils'
import Form from '@rjsf/core'
import { FieldTemplateProps, IconButtonProps, ObjectFieldTemplateProps, RJSFSchema, UiSchema } from '@rjsf/utils'
import validator from '@rjsf/validator-ajv8'
import { Button } from '@/components/ui/button'
//import { Form, FormControl, FormDescription, FormField, FormItem, FormLabel, FormMessage } from '@/components/ui/form'
//Using ShadCN, create a form that allows submitting record types mapping string to string
import { Input } from '@/components/ui/input'
import { PlusIcon, Trash2Icon } from 'lucide-react'
import z from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'
import { useForm } from 'react-hook-form'

const showSettingsAtom = atom(true)

const settingStorageAtom = createJSONStorage()

const envvarAtom = atomWithStorage('envvars', {}, settingStorageAtom)

const schema: RJSFSchema = {
  title: 'Environment variables',
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
  default: [
    { key: 'OPENAI_API_KEY', value: 'this-is-openai' },
    { key: 'ANTHROPIC_API_KEY', value: 'this-is-anthropic' },
  ],
}

const uiSchema: UiSchema = {
  envvars: {
    items: {
      key: {
        'ui:classNames': 'bg-blue-700',
        'ui:FieldTemplate': EnvvarKeyTemplate,
      },
      value: {
        'ui:classNames': 'bg-purple-700',
        'ui:FieldTemplate': EnvvarValueTemplate,
      },
    },
  },
}

function EnvvarKeyTemplate(props: FieldTemplateProps) {
  const { id, classNames, style, label, help, required, description, errors, children } = props
  return (
    <div className={classNames} style={style}>
      <div>envkey</div>
      <label htmlFor={id}>
        {label}
        {required ? '*' : null}
      </label>
      {description}
      {children}
      {errors}
      {help}
    </div>
  )
}

function EnvvarValueTemplate(props: FieldTemplateProps) {
  const { id, classNames, style, label, help, required, description, errors, children } = props
  return (
    <div className={classNames} style={style}>
      <div>envvalue</div>
      <label htmlFor={id}>
        {label}
        {required ? '*' : null}
      </label>
      {description}
      {children}
      {errors}
      {help}
    </div>
  )
}

const EnvVarFieldTemplate = (props: ObjectFieldTemplateProps) => {
  return (
    <div>
      <p>envvar template</p>
      {props.title}
      {props.description}
      {props.properties.map((element) => (
        <div className='property-wrapper'>{element.content}</div>
      ))}
    </div>
  )
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
        className='!flex flex-col !px-0 !py-0 bg-red-700 h-fit !max-w-[48px] ml-auto'
        style={{
          flex: 'none',
        }}
      >
        <Trash2Icon size={14} />
      </Button>
    </div>
  )
}

//const EnvvarSchema = z.record(z.string(), z.string())
//
//export function EnvvarForm() {
//  // 1. Define your form.
//  const form = useForm<z.infer<typeof EnvvarSchema>>({
//    resolver: zodResolver(EnvvarSchema),
//    defaultValues: {},
//  })
//
//  // 2. Define a submit handler.
//  function onSubmit(values: z.infer<typeof EnvvarSchema>) {
//    // Do something with the form values.
//    // ✅ This will be type-safe and validated.
//    console.log(values)
//  }
//
//  return (
//    <Form {...form}>
//      <form onSubmit={form.handleSubmit(onSubmit)} className='space-y-8'>
//        <FormField
//          control={form.control}
//          name='envvar'
//          render={({ field }) => (
//            <FormItem>
//              <FormLabel>Username</FormLabel>
//              <FormControl>
//                <Input placeholder='shadcn' {...field} />
//              </FormControl>
//              <FormDescription>This is your public display name.</FormDescription>
//              <FormMessage />
//            </FormItem>
//          )}
//        />
//        <Button type='submit'>Submit</Button>
//      </form>
//    </Form>
//  )
//}

export const SettingsDialog: React.FC = () => {
  const [showSettings, setShowSettings] = useAtom(showSettingsAtom)
  const duplicate = false

  const [envvars, setEnvvars] = useAtom(envvarAtom)

  //<DialogTrigger asChild={true}>{children}</DialogTrigger>
  return (
    <Dialog open={showSettings} onOpenChange={setShowSettings}>
      <DialogContent className='overflow-y-scroll max-h-screen bg-vscode-editorWidget-background border-vscode-textSeparator-foreground overflow-x-clip'>
        <DialogHeader className='flex flex-row gap-x-4 items-center'>
          <DialogTitle className='text-xs font-semibold'>{duplicate ? 'Duplicate test' : 'Edit test'}</DialogTitle>

          <div className='flex flex-row gap-x-2 items-center pb-1'>
            <div>renaming tests not supported right now</div>
          </div>
        </DialogHeader>
        <p> contents of the form</p>
        <Form
          schema={schema}
          uiSchema={uiSchema}
          validator={validator}
          onSubmit={(d) => console.log('rjsf form data', d)}
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

export { showSettingsAtom, settingStorageAtom }
