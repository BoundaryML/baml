import { uniqueNamesGenerator, Config, adjectives, colors, animals } from 'unique-names-generator'
import { Button } from '../../components/ui/button'
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '../../components/ui/dialog'
import { vscode } from '../../utils/vscode'
import { ParserDatabase, StringSpan, TestRequest } from '@baml/common'
import Form, { getDefaultRegistry } from '@rjsf/core'
import validator from '@rjsf/validator-ajv8'
import { VSCodeButton, VSCodeProgressRing, VSCodeTextArea, VSCodeTextField } from '@vscode/webview-ui-toolkit/react'
import { Copy, Edit2, FileJson2, Save, Play, PlusIcon, Trash2, XIcon } from 'lucide-react'
import React, { ChangeEvent, FocusEvent, useCallback, useContext, useEffect, useMemo, useState } from 'react'
import { ASTContext } from '../ASTProvider'
import TypeComponent from '../TypeComponent'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '../../components/ui/tooltip'
import {
  ArrayFieldTemplateItemType,
  ArrayFieldTitleProps,
  BaseInputTemplateProps,
  FieldTemplateProps,
  IconButtonProps,
  ObjectFieldTemplateProps,
  UiSchema,
  ariaDescribedByIds,
  examplesId,
  getInputProps,
  titleId,
} from '@rjsf/utils'

function BaseInputTemplate(props: BaseInputTemplateProps) {
  const {
    id,
    name, // remove this from ...rest
    value,
    readonly,
    disabled,
    autofocus,
    onBlur,
    onFocus,
    onChange,
    onChangeOverride,
    options,
    schema,
    uiSchema,
    formContext,
    registry,
    rawErrors,
    type,
    hideLabel, // remove this from ...rest
    hideError, // remove this from ...rest
    ...rest
  } = props

  // Note: since React 15.2.0 we can't forward unknown element attributes, so we
  // exclude the "options" and "schema" ones here.
  if (!id) {
    console.log('No id for', props)
    throw new Error(`no id for props ${JSON.stringify(props)}`)
  }
  const inputProps = {
    ...rest,
    ...getInputProps(schema, type, options),
  }

  let inputValue
  if (inputProps.type === 'number' || inputProps.type === 'integer') {
    inputValue = value || value === 0 ? value : ''
  } else {
    inputValue = value == null ? '' : value
  }

  const _onChange = useCallback(
    ({ target: { value } }: ChangeEvent<HTMLInputElement>) => onChange(value === '' ? options.emptyValue : value),
    [onChange, options],
  )
  const _onBlur = useCallback(({ target: { value } }: FocusEvent<HTMLInputElement>) => onBlur(id, value), [onBlur, id])
  const _onFocus = useCallback(
    ({ target: { value } }: FocusEvent<HTMLInputElement>) => onFocus(id, value),
    [onFocus, id],
  )

  const length = Object.keys(registry.rootSchema?.definitions ?? {}).length

  const input =
    inputProps.type === 'number' || inputProps.type === 'integer' ? (
      <input
        id={id}
        name={id}
        className="max-w-[100px] rounded-sm bg-vscode-input-background text-vscode-input-foreground"
        readOnly={readonly}
        disabled={disabled}
        autoFocus={autofocus}
        value={inputValue}
        {...inputProps}
        list={schema.examples ? examplesId(id) : undefined}
        onChange={onChangeOverride || _onChange}
        onBlur={_onBlur}
        onFocus={_onFocus}
        aria-describedby={ariaDescribedByIds(id, !!schema.examples)}
      />
    ) : (
      <textarea
        id={id}
        name={id}
        rows={Math.max(
          1,
          inputValue
            .split('\n')
            .map((line: string) => Math.max(1, line.length / 42))
            .reduce((a: number, b: number) => a + b, 0),
        )}
        className="w-[90%] px-1 rounded-sm bg-vscode-input-background text-vscode-input-foreground"
        readOnly={readonly}
        disabled={disabled}
        autoFocus={autofocus}
        value={inputValue}
        {...inputProps}
        // list={schema.examples ? examplesId(id) : undefined}
        onChange={(onChangeOverride as any) || _onChange}
        onBlur={_onBlur as any}
        onFocus={_onFocus as any}
        aria-describedby={ariaDescribedByIds(id, !!schema.examples)}
      />
    )

  return (
    <div className="flex flex-col w-full gap-y-1">
      {input}
      {Array.isArray(schema.examples) && (
        <datalist key={`datalist_${id}`} id={examplesId(id)}>
          {(schema.examples as string[])
            .concat(schema.default && !schema.examples.includes(schema.default) ? ([schema.default] as string[]) : [])
            .map((example: any) => {
              return <option key={example} value={example} />
            })}
        </datalist>
      )}
    </div>
  )
}

const typeLabel = (schema: any): string => {
  if (schema.type === "array") {
    return `${typeLabel(schema.items)}[]`
  }
  return schema.title || schema.type
};

function FieldTemplate(props: FieldTemplateProps) {
  const { id, classNames, style, label, displayLabel, help, required, hidden, description, errors, children } = props

  if (hidden) {
    return <div className="hidden">{children}</div>
  }

  return (
    <div className={classNames + ' ml-2 w-full'} style={style}>
      <>
        {props.schema.type === 'boolean' ? null : (
          <label htmlFor={id} className="flex flex-row items-center gap-x-3">
            <div className={props.schema.type === 'object' ? ' font-bold text-sm' : ' text-xs'}>
              {label.split('-').at(-1)}
            </div>
            <div className={'text-vscode-textSeparator-foreground text-xs font-mono'}>
              {typeLabel(props.schema)}
            </div>
          </label>
        )}
      </>

      {description}
      <div className="flex flex-row items-center w-full">{children}</div>
      {errors}
      {help}
    </div>
  )
}

function ObjectFieldTemplate(props: ObjectFieldTemplateProps) {
  return (
    <div className="w-full">
      {/* <div className="py-2">{props.title}</div> */}
      {props.description}
      <div className="flex flex-col w-full py-1 gap-y-2">
        {props.properties.map((element) => (
          <div className="w-full property-wrapper text-vscode-input-foreground">{element.content}</div>
        ))}
      </div>
    </div>
  )
}

function AddButton(props: IconButtonProps) {
  const { icon, iconType, ...btnProps } = props
  return (
    <Button
      variant="ghost"
      size="icon"
      {...btnProps}
      className="flex flex-row items-center p-1 text-xs w-fit h-fit gap-x-2 hover:bg-vscode-descriptionForeground"
    >
      <PlusIcon size={14} /> <div>Add item</div>
    </Button>
  )
}

function RemoveButton(props: IconButtonProps) {
  const { icon, iconType, ...btnProps } = props
  return (
    <div className="flex w-fit h-fit">
      <Button
        {...btnProps}
        size={'icon'}
        className="!flex flex-col !px-0 !py-0 bg-red-700 h-fit !max-w-[48px] ml-auto"
        style={{
          flex: 'none',
        }}
      >
        <XIcon size={14} />
      </Button>
    </div>
  )
}

function SubmitButton(props: IconButtonProps) {
  const { icon, iconType, ...btnProps } = props
  return (
    <div className="flex items-end justify-end w-full ml-auto h-fit">
      <Button
        {...btnProps}
        className="px-3 py-2 rounded-none bg-vscode-button-background text-vscode-button-foreground w-fit h-fit hover:bg-vscode-button-background hover:opacity-75"
        style={{
          flex: 'none',
        }}
      >
        Submit
      </Button>
    </div>
  )
}

function ArrayFieldItemTemplate(props: ArrayFieldTemplateItemType) {
  const { children, className } = props
  return (
    <div>
      <div className={`${className} flex flex-row ml-0 py-1 text-xs text-vscode-descriptionForeground`}>
        {props.hasRemove && (
          <div className="flex ml-auto w-fit h-fit">
            <Button
              onClick={props.onDropIndexClick(props.index)}
              disabled={props.disabled || props.readonly}
              size={'icon'}
              className="p-1 bg-transparent w-fit h-fit hover:bg-red-700"
              style={{
                flex: 'none',
              }}
            >
              <XIcon size={14} />
            </Button>
          </div>
        )}
        {children}
      </div>
    </div>
  )
}

function ArrayFieldTitleTemplate(props: ArrayFieldTitleProps) {
  const { title, idSchema } = props
  const id = titleId(idSchema)
  return null
  // return (
  //   <div id={id} className="text-xs">
  //     {title}
  //   </div>
  // )
}

export const TEMPLATES = {
  BaseInputTemplate,
  FieldTemplate,
  ObjectFieldTemplate,
  ButtonTemplates: {
    AddButton,
    RemoveButton,
    SubmitButton,
  },
  ArrayFieldTitleTemplate,
  ArrayFieldItemTemplate,
};