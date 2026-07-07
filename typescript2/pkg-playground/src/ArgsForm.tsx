/**
 * Dynamic args form for the Run tab.
 *
 * Renders one typed widget per function parameter from the `ParamSchema` tree
 * on `FunctionInfo`, dispatching recursively on `FieldSchema.type` (the same
 * shape as ValueRenderer's type dispatch). Fully controlled: the single source
 * of truth is the `value` record, which the host serializes into the existing
 * `argsJson` pipeline on every edit. Value/marker semantics live in
 * args-form-model.ts.
 *
 * Nodes the form can't render typed (unsupported types, media, recursion
 * cut-points) degrade to a per-field raw-JSON textarea.
 */

import { useState, type FC, type ReactNode } from 'react';
import { ChevronRight, Plus, Trash2 } from 'lucide-react';

import {
  activeUnionVariant,
  defaultValueForSchema,
  enumValue,
  enumVariantOf,
  isRawJsonSchema,
  schemaLabel,
} from './args-form-model';
import { Button } from './components/ui/button';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from './components/ui/collapsible';
import { Input } from './components/ui/input';
import { Select } from './components/ui/select';
import { Switch } from './components/ui/switch';
import { Textarea } from './components/ui/textarea';
import { ToggleGroup } from './components/ui/toggle-group';
import { cn } from './lib/utils';
import type { FieldSchema, ParamSchema } from './worker-protocol';

/** Enums up to this size render as toggle chips; larger ones as a dropdown. */
const ENUM_TOGGLE_MAX = 5;
/** Class sections nested this deep start collapsed (ValueRenderer convention). */
const AUTO_COLLAPSE_DEPTH = 2;

export interface ArgsFormProps {
  params: ParamSchema[];
  /** Parsed `argsJson` object; surplus keys are preserved by edits. */
  value: Record<string, unknown>;
  onChange: (next: Record<string, unknown>) => void;
  disabled?: boolean;
}

export const ArgsForm: FC<ArgsFormProps> = ({
  params,
  value,
  onChange,
  disabled,
}) => {
  if (params.length === 0) {
    return (
      <div className="text-xs text-vsc-description py-1">
        This function takes no arguments.
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-1.5">
      {params.map((param) => (
        <ParamRow
          key={param.name}
          param={param}
          value={value[param.name]}
          present={param.name in value}
          disabled={disabled}
          onChange={(v) => onChange({ ...value, [param.name]: v })}
          onOmit={() => {
            const { [param.name]: _omitted, ...rest } = value;
            onChange(rest);
          }}
        />
      ))}
    </div>
  );
};

const ParamRow: FC<{
  param: ParamSchema;
  value: unknown;
  present: boolean;
  disabled?: boolean;
  onChange: (v: unknown) => void;
  onOmit: () => void;
}> = ({ param, value, present, disabled, onChange, onOmit }) => {
  const omitted = param.hasDefault && !present;
  return (
    <div className="flex flex-col gap-0.5">
      <div className="flex items-center gap-1.5">
        <span className="font-vsc-mono text-xs text-foreground">
          {param.name}
        </span>
        <span className="font-vsc-mono text-[10px] text-vsc-text-faint">
          {schemaLabel(param.schema)}
        </span>
        {param.hasDefault && (
          <label className="ml-auto flex items-center gap-1 text-[10px] text-vsc-description">
            set
            <Switch
              checked={!omitted}
              disabled={disabled}
              onCheckedChange={(on) =>
                on ? onChange(defaultValueForSchema(param.schema)) : onOmit()
              }
            />
          </label>
        )}
      </div>
      {omitted ? (
        <div className="text-[10px] text-vsc-text-faint pl-0.5">
          omitted — uses the declared default
        </div>
      ) : (
        <FieldInput
          schema={param.schema}
          value={value}
          onChange={onChange}
          depth={0}
          disabled={disabled}
        />
      )}
    </div>
  );
};

interface FieldInputProps {
  schema: FieldSchema;
  value: unknown;
  onChange: (v: unknown) => void;
  depth: number;
  disabled?: boolean;
}

/** Recursive schema-directed widget dispatch. */
const FieldInput: FC<FieldInputProps> = (props) => {
  const { schema } = props;
  if (isRawJsonSchema(schema)) {
    return <RawJsonField {...props} />;
  }
  switch (schema.type) {
    case 'string':
      return <StringField {...props} />;
    case 'int':
    case 'bigint':
      return <NumberField {...props} integer />;
    case 'float':
      return <NumberField {...props} />;
    case 'bool':
      return <BoolField {...props} />;
    case 'null':
      return <span className="font-vsc-mono text-xs text-vsc-text-faint">null</span>;
    case 'literal':
      return (
        <span className="font-vsc-mono text-xs text-vsc-description">
          {JSON.stringify(schema.value)}
        </span>
      );
    case 'enum':
      return <EnumField {...props} schema={schema} />;
    case 'class':
      return <ClassSection {...props} schema={schema} />;
    case 'list':
      return <ListField {...props} schema={schema} />;
    case 'map':
      return <MapField {...props} schema={schema} />;
    case 'optional':
      return <OptionalField {...props} schema={schema} />;
    case 'union':
      return <UnionField {...props} schema={schema} />;
    // media/unsupported/recursive-class are handled by isRawJsonSchema above.
    default:
      return <RawJsonField {...props} />;
  }
};

/** Draft text that resets whenever the canonical external text changes. */
function useDraft(canonical: string) {
  const [draft, setDraft] = useState(canonical);
  const [prev, setPrev] = useState(canonical);
  if (canonical !== prev) {
    setPrev(canonical);
    setDraft(canonical);
  }
  return [draft, setDraft] as const;
}

const StringField: FC<FieldInputProps> = ({ value, onChange, disabled }) => (
  <Input
    className="h-7 text-xs font-vsc-mono"
    value={typeof value === 'string' ? value : ''}
    placeholder="text"
    disabled={disabled}
    onChange={(e) => onChange(e.target.value)}
  />
);

const NumberField: FC<FieldInputProps & { integer?: boolean }> = ({
  value,
  onChange,
  disabled,
  integer,
}) => {
  const canonical =
    typeof value === 'number' || typeof value === 'bigint'
      ? String(value)
      : '';
  const [draft, setDraft] = useDraft(canonical);
  const parsed = draft.trim() === '' ? undefined : Number(draft);
  const valid =
    parsed === undefined ||
    (Number.isFinite(parsed) && (!integer || Number.isInteger(parsed)));
  return (
    <Input
      className="h-7 text-xs font-vsc-mono"
      inputMode={integer ? 'numeric' : 'decimal'}
      value={draft}
      placeholder={integer ? '0' : '0.0'}
      disabled={disabled}
      aria-invalid={!valid}
      onChange={(e) => {
        const text = e.target.value;
        setDraft(text);
        const num = text.trim() === '' ? undefined : Number(text);
        if (num === undefined) {
          onChange(undefined);
        } else if (
          Number.isFinite(num) &&
          (!integer || Number.isInteger(num))
        ) {
          onChange(num);
        }
      }}
    />
  );
};

const BoolField: FC<FieldInputProps> = ({ value, onChange, disabled }) => (
  <Switch
    checked={value === true}
    disabled={disabled}
    onCheckedChange={(checked) => onChange(checked)}
  />
);

const EnumField: FC<
  FieldInputProps & { schema: Extract<FieldSchema, { type: 'enum' }> }
> = ({ schema, value, onChange, disabled }) => {
  const current = enumVariantOf(value);
  if (schema.values.length <= ENUM_TOGGLE_MAX) {
    return (
      <ToggleGroup
        size="sm"
        value={current ?? ''}
        options={schema.values.map((v) => ({ value: v, label: v }))}
        onValueChange={(v) => {
          if (!disabled) onChange(enumValue(schema.name, v));
        }}
      />
    );
  }
  return (
    <Select
      className="max-w-[240px]"
      value={current ?? ''}
      disabled={disabled}
      onChange={(e) => onChange(enumValue(schema.name, e.target.value))}
    >
      {current === undefined && <option value="">select…</option>}
      {schema.values.map((v) => (
        <option key={v} value={v}>
          {v}
        </option>
      ))}
    </Select>
  );
};

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

const ClassSection: FC<
  FieldInputProps & { schema: Extract<FieldSchema, { type: 'class' }> }
> = ({ schema, value, onChange, depth, disabled }) => {
  const [open, setOpen] = useState(depth < AUTO_COLLAPSE_DEPTH);
  const obj = isPlainObject(value) ? value : {};
  const setField = (name: string, v: unknown) =>
    onChange({ ...obj, $baml: { type: schema.name }, [name]: v });
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className="flex items-center gap-1 cursor-pointer text-xs text-vsc-description hover:text-foreground">
        <ChevronRight
          size={12}
          className={cn('transition-transform', open && 'rotate-90')}
        />
        <span className="font-vsc-mono">{schemaLabel(schema)}</span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-1 border-l border-vsc-border ml-1.5 pl-2.5 pt-1">
          {schema.fields.map((field) => (
            <div key={field.name} className="flex flex-col gap-0.5">
              <div className="flex items-center gap-1.5">
                <span className="font-vsc-mono text-xs text-foreground">
                  {field.name}
                </span>
                <span className="font-vsc-mono text-[10px] text-vsc-text-faint">
                  {schemaLabel(field.schema)}
                </span>
              </div>
              <FieldInput
                schema={field.schema}
                value={obj[field.name]}
                onChange={(v) => setField(field.name, v)}
                depth={depth + 1}
                disabled={disabled}
              />
            </div>
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
};

const ListField: FC<
  FieldInputProps & { schema: Extract<FieldSchema, { type: 'list' }> }
> = ({ schema, value, onChange, depth, disabled }) => {
  const items = Array.isArray(value) ? value : [];
  return (
    <div className="flex flex-col gap-1">
      {items.map((item, i) => (
        <div key={i} className="flex items-start gap-1">
          <div className="flex-1 min-w-0">
            <FieldInput
              schema={schema.item}
              value={item}
              onChange={(v) =>
                onChange(items.map((cur, j) => (j === i ? v : cur)))
              }
              depth={depth + 1}
              disabled={disabled}
            />
          </div>
          <Button
            variant="ghost"
            size="icon-xs"
            className="text-vsc-red shrink-0"
            disabled={disabled}
            aria-label="Remove item"
            onClick={() => onChange(items.filter((_, j) => j !== i))}
          >
            <Trash2 />
          </Button>
        </div>
      ))}
      <Button
        variant="ghost"
        size="xs"
        className="self-start text-vsc-link"
        disabled={disabled}
        onClick={() =>
          onChange([...items, defaultValueForSchema(schema.item)])
        }
      >
        <Plus /> add item
      </Button>
    </div>
  );
};

/** Per-row key editor: commits a rename only when it wouldn't collide. */
const MapKeyInput: FC<{
  mapKey: string;
  siblingKeys: string[];
  disabled?: boolean;
  onRename: (next: string) => void;
}> = ({ mapKey, siblingKeys, disabled, onRename }) => {
  const [draft, setDraft] = useDraft(mapKey);
  const collides = draft !== mapKey && siblingKeys.includes(draft);
  return (
    <Input
      className="h-7 text-xs font-vsc-mono w-[130px] shrink-0"
      value={draft}
      placeholder="key"
      disabled={disabled}
      aria-invalid={collides}
      onChange={(e) => {
        setDraft(e.target.value);
        if (
          e.target.value !== mapKey &&
          !siblingKeys.includes(e.target.value)
        ) {
          onRename(e.target.value);
        }
      }}
    />
  );
};

const MapField: FC<
  FieldInputProps & { schema: Extract<FieldSchema, { type: 'map' }> }
> = ({ schema, value, onChange, depth, disabled }) => {
  const obj =
    isPlainObject(value) && !('$baml' in value)
      ? value
      : ({} as Record<string, unknown>);
  const entries = Object.entries(obj);
  const rebuild = (
    mapped: (entry: [string, unknown], i: number) => [string, unknown] | null,
  ) =>
    onChange(
      Object.fromEntries(
        entries.map(mapped).filter((e): e is [string, unknown] => e !== null),
      ),
    );
  const freshKey = () => {
    let i = entries.length + 1;
    while (`key${i}` in obj) i += 1;
    return `key${i}`;
  };
  return (
    <div className="flex flex-col gap-1">
      {entries.map(([k, v], i) => (
        <div key={i} className="flex items-start gap-1">
          <MapKeyInput
            mapKey={k}
            siblingKeys={entries.map(([sk]) => sk)}
            disabled={disabled}
            onRename={(nk) => rebuild((e, j) => (j === i ? [nk, e[1]] : e))}
          />
          <div className="flex-1 min-w-0">
            <FieldInput
              schema={schema.value}
              value={v}
              onChange={(nv) => rebuild((e, j) => (j === i ? [e[0], nv] : e))}
              depth={depth + 1}
              disabled={disabled}
            />
          </div>
          <Button
            variant="ghost"
            size="icon-xs"
            className="text-vsc-red shrink-0"
            disabled={disabled}
            aria-label="Remove entry"
            onClick={() => rebuild((e, j) => (j === i ? null : e))}
          >
            <Trash2 />
          </Button>
        </div>
      ))}
      <Button
        variant="ghost"
        size="xs"
        className="self-start text-vsc-link"
        disabled={disabled}
        onClick={() =>
          onChange({
            ...obj,
            [freshKey()]: defaultValueForSchema(schema.value),
          })
        }
      >
        <Plus /> add entry
      </Button>
    </div>
  );
};

const OptionalField: FC<
  FieldInputProps & { schema: Extract<FieldSchema, { type: 'optional' }> }
> = ({ schema, value, onChange, depth, disabled }) => {
  const isSet = value !== null && value !== undefined;
  return (
    <div className="flex flex-col gap-1">
      <label className="flex items-center gap-1.5 text-[10px] text-vsc-description">
        <Switch
          checked={isSet}
          disabled={disabled}
          onCheckedChange={(on) =>
            onChange(on ? defaultValueForSchema(schema.inner) : null)
          }
        />
        {isSet ? 'set' : 'null'}
      </label>
      {isSet && (
        <FieldInput
          schema={schema.inner}
          value={value}
          onChange={onChange}
          depth={depth}
          disabled={disabled}
        />
      )}
    </div>
  );
};

const UnionField: FC<
  FieldInputProps & { schema: Extract<FieldSchema, { type: 'union' }> }
> = ({ schema, value, onChange, depth, disabled }) => {
  const detected = activeUnionVariant(value, schema.variants);
  const [chosen, setChosen] = useState(0);
  const active = detected >= 0 ? detected : chosen;
  return (
    <div className="flex flex-col gap-1">
      <ToggleGroup
        size="sm"
        value={String(active)}
        options={schema.variants.map((v, i) => ({
          value: String(i),
          label: schemaLabel(v),
        }))}
        onValueChange={(v) => {
          if (disabled) return;
          const index = Number(v);
          setChosen(index);
          onChange(defaultValueForSchema(schema.variants[index]));
        }}
      />
      {schema.variants[active] && (
        <FieldInput
          schema={schema.variants[active]}
          value={value}
          onChange={onChange}
          depth={depth}
          disabled={disabled}
        />
      )}
    </div>
  );
};

/** Fallback editor for nodes without a typed widget: a JSON textarea that
 *  commits on every parseable edit and flags unparseable drafts. */
const RawJsonField: FC<FieldInputProps> = ({
  schema,
  value,
  onChange,
  disabled,
}) => {
  const canonical = value === undefined ? '' : JSON.stringify(value);
  const [draft, setDraft] = useDraft(canonical);
  let valid = true;
  if (draft.trim() !== '') {
    try {
      JSON.parse(draft);
    } catch {
      valid = false;
    }
  }
  return (
    <Textarea
      className="min-h-[28px] px-2 py-1 font-vsc-mono text-xs resize-y"
      rows={1}
      value={draft}
      placeholder={`JSON (${schemaLabel(schema)})`}
      disabled={disabled}
      aria-invalid={!valid}
      onChange={(e) => {
        const text = e.target.value;
        setDraft(text);
        if (text.trim() === '') {
          onChange(undefined);
          return;
        }
        try {
          onChange(JSON.parse(text));
        } catch {
          // keep the draft; the invalid style flags it
        }
      }}
    />
  );
};
