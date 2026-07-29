/**
 * Dynamic args form for the Run tab.
 *
 * Renders one typed widget per function parameter from the `ParamSchema` tree
 * on `FunctionInfo`, dispatching recursively on `FieldSchema.type` (the same
 * shape as ValueRenderer's type dispatch). Named types are `ref`s resolved
 * against the per-project type table (`ProjectUpdate.types`), provided via
 * context; class sections resolve lazily on expand (Radix unmounts closed
 * `CollapsibleContent`), which is what makes recursive types fully typed at
 * every depth the user opens. Fully controlled: the single source of truth is
 * the `value` record, which the host serializes into the existing `argsJson`
 * pipeline on every edit. Value/marker semantics live in args-form-model.ts.
 *
 * Nodes the form can't render typed (unsupported types, media, dangling refs)
 * degrade to a per-field raw-JSON textarea.
 */

import {
  createContext,
  useContext,
  useMemo,
  useState,
  type FC,
  type ReactNode,
} from 'react';
import { ChevronRight, Plus, Trash2 } from 'lucide-react';

import {
  activeUnionVariant,
  defaultValueForSchema,
  enumValue,
  enumVariantOf,
  isPlainObject,
  isRawJsonSchema,
  resolveRef,
  schemaLabel,
  typeLookupFrom,
  valueMatchesSchema,
  type TypeLookup,
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
import type {
  FieldSchema,
  FieldSchemaField,
  ParamSchema,
  TypeSchema,
} from './worker-protocol';

/** Enums up to this size render as toggle chips; larger ones as a dropdown. */
const ENUM_TOGGLE_MAX = 5;
/** Class sections nested this deep start collapsed (ValueRenderer convention). */
const AUTO_COLLAPSE_DEPTH = 2;

const TypeLookupContext = createContext<TypeLookup>(() => undefined);

export interface ArgsFormProps {
  params: ParamSchema[];
  /** Per-project named-type table the schemas' refs resolve against. */
  types?: Record<string, TypeSchema>;
  /** Parsed `argsJson` object; surplus keys are preserved by edits. */
  value: Record<string, unknown>;
  onChange: (next: Record<string, unknown>) => void;
}

export const ArgsForm: FC<ArgsFormProps> = ({
  params,
  types,
  value,
  onChange,
}) => {
  const lookup = useMemo(() => typeLookupFrom(types), [types]);
  if (params.length === 0) {
    return (
      <div className="text-xs text-vsc-description py-1">
        This function takes no arguments.
      </div>
    );
  }
  return (
    <TypeLookupContext.Provider value={lookup}>
      <div className="flex flex-col gap-1.5">
        {params.map((param) => (
          <ParamRow
            key={param.name}
            param={param}
            value={value[param.name]}
            present={param.name in value}
            onChange={(v) => onChange({ ...value, [param.name]: v })}
            onOmit={() => {
              const { [param.name]: _omitted, ...rest } = value;
              onChange(rest);
            }}
          />
        ))}
      </div>
    </TypeLookupContext.Provider>
  );
};

/** Shared field header: name plus a faint type label. */
const FieldLabel: FC<{ name: string; schema: FieldSchema; extra?: ReactNode }> =
  ({ name, schema, extra }) => (
    <div className="flex items-center gap-1.5">
      <span className="font-vsc-mono text-xs text-foreground">{name}</span>
      <span className="font-vsc-mono text-[10px] text-vsc-text-faint">
        {schemaLabel(schema)}
      </span>
      {extra}
    </div>
  );

const ParamRow: FC<{
  param: ParamSchema;
  value: unknown;
  present: boolean;
  onChange: (v: unknown) => void;
  onOmit: () => void;
}> = ({ param, value, present, onChange, onOmit }) => {
  const lookup = useContext(TypeLookupContext);
  const omitted = param.hasDefault && !present;
  return (
    <div className="flex flex-col gap-0.5">
      <FieldLabel
        name={param.name}
        schema={param.schema}
        extra={
          param.hasDefault && (
            <label
              className="flex items-center gap-1 text-[10px] text-vsc-description"
              htmlFor={`override-${param.name}`}
            >
              <Switch
                id={`override-${param.name}`}
                aria-label={`Override ${param.name}`}
                checked={!omitted}
                onCheckedChange={(on) =>
                  on
                    ? onChange(defaultValueForSchema(param.schema, lookup))
                    : onOmit()
                }
              />
              override
            </label>
          )
        }
      />
      <fieldset
        className="m-0 min-w-0 border-0 p-0 disabled:opacity-60"
        disabled={omitted}
      >
        <FieldInput
          schema={param.schema}
          value={value}
          onChange={onChange}
          depth={0}
          disabled={omitted}
          placeholder={param.defaultExpression}
        />
      </fieldset>
    </div>
  );
};

interface FieldInputProps {
  schema: FieldSchema;
  value: unknown;
  onChange: (v: unknown) => void;
  depth: number;
  /** Whether this field is inside a disabled default-argument control. */
  disabled?: boolean;
  /** Placeholder for a top-level declared default expression. */
  placeholder?: string;
  /** Ref names already unwrapped without descending into a child value —
   *  guards self-referential alias schemas (`type A = A | int` compiles
   *  clean) from recursing the render unboundedly. Same-value hops
   *  (ref→alias target, union variant, optional inner) thread it through;
   *  descending into a child value (class field, list item, map entry)
   *  resets it, so legitimately recursive types still render at every
   *  depth the user opens. */
  refPath?: readonly string[];
}

/** Recursive schema-directed widget dispatch. */
const FieldInput: FC<FieldInputProps> = (props) => {
  const lookup = useContext(TypeLookupContext);
  const { schema, refPath = [] } = props;
  if (isRawJsonSchema(schema, lookup)) {
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
    case 'enumVariant':
      return (
        <EnumField
          {...props}
          enumName={schema.name}
          values={[schema.value]}
        />
      );
    case 'ref': {
      // isRawJsonSchema handled dangling refs above, so this resolves.
      const resolved = resolveRef(schema.name, lookup);
      if (resolved === undefined) return <RawJsonField {...props} />;
      if (resolved.kind === 'enum') {
        return (
          <EnumField
            {...props}
            enumName={resolved.name}
            values={resolved.values}
          />
        );
      }
      if (resolved.kind === 'schema') {
        if (refPath.includes(schema.name)) {
          return <RawJsonField {...props} />;
        }
        return (
          <FieldInput
            {...props}
            schema={resolved.schema}
            refPath={[...refPath, schema.name]}
          />
        );
      }
      return (
        <ClassSection
          {...props}
          typeName={resolved.name}
          fields={resolved.fields}
        />
      );
    }
    case 'list':
      return <ListField {...props} schema={schema} />;
    case 'map':
      return <MapField {...props} schema={schema} />;
    case 'optional':
      return <OptionalField {...props} schema={schema} />;
    case 'union':
      return <UnionField {...props} schema={schema} />;
    // media/unsupported/unknown tags are handled by isRawJsonSchema above.
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

const StringField: FC<FieldInputProps> = ({
  value,
  onChange,
  placeholder = 'text',
}) => (
  <Input
    className="h-7 text-xs font-vsc-mono"
    value={typeof value === 'string' ? value : ''}
    placeholder={placeholder}
    onChange={(e) => onChange(e.target.value)}
  />
);

const NumberField: FC<FieldInputProps & { integer?: boolean }> = ({
  value,
  onChange,
  integer,
  disabled,
  placeholder,
}) => {
  const canonical =
    typeof value === 'number' || typeof value === 'bigint'
      ? String(value)
      : '';
  const [draft, setDraft] = useDraft(canonical);
  const parse = (text: string): number | null => {
    const trimmed = text.trim();
    if (trimmed === '') return null;
    const num = Number(trimmed);
    return Number.isFinite(num) && (!integer || Number.isInteger(num))
      ? num
      : null;
  };
  // An empty or unparseable draft is an error state, not a deletion: the last
  // committed value stays in place so required keys never silently drop out
  // of argsJson.
  return (
    <Input
      className="h-7 text-xs font-vsc-mono"
      inputMode={integer ? 'numeric' : 'decimal'}
      value={draft}
      placeholder={placeholder ?? (integer ? '0' : '0.0')}
      aria-invalid={!disabled && parse(draft) === null}
      onChange={(e) => {
        setDraft(e.target.value);
        const num = parse(e.target.value);
        if (num !== null) onChange(num);
      }}
    />
  );
};

const BoolField: FC<FieldInputProps> = ({ value, onChange }) => (
  <Switch
    checked={value === true}
    onCheckedChange={(checked) => onChange(checked)}
  />
);

const EnumField: FC<
  FieldInputProps & { enumName: string; values: string[] }
> = ({ enumName, values, value, onChange }) => {
  const current = enumVariantOf(value);
  if (values.length <= ENUM_TOGGLE_MAX) {
    return (
      <ToggleGroup
        size="sm"
        value={current ?? ''}
        options={values.map((v) => ({ value: v, label: v }))}
        onValueChange={(v) => onChange(enumValue(enumName, v))}
      />
    );
  }
  return (
    <div className="max-w-[240px]">
      <Select
        value={current ?? ''}
        onChange={(e) => {
          // The empty value is the "select…" placeholder, not a variant.
          if (e.target.value === '') return;
          onChange(enumValue(enumName, e.target.value));
        }}
      >
        {current === undefined && <option value="">select…</option>}
        {values.map((v) => (
          <option key={v} value={v}>
            {v}
          </option>
        ))}
      </Select>
    </div>
  );
};

const ClassSection: FC<
  FieldInputProps & { typeName: string; fields: FieldSchemaField[] }
> = ({ typeName, fields, schema, value, onChange, depth }) => {
  const [open, setOpen] = useState(depth < AUTO_COLLAPSE_DEPTH);
  const obj = isPlainObject(value) ? value : {};
  const setField = (name: string, v: unknown) =>
    onChange({ ...obj, $baml: { type: typeName }, [name]: v });
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className="flex items-center gap-1 cursor-pointer text-xs text-vsc-description hover:text-foreground">
        <ChevronRight
          size={12}
          className={cn('transition-transform', open && 'rotate-90')}
        />
        <span className="font-vsc-mono">{schemaLabel(schema)}</span>
      </CollapsibleTrigger>
      {/* Closed content is unmounted — nested refs (including back into this
          class) only resolve and render when the user opens the section. */}
      <CollapsibleContent>
        <div className="flex flex-col gap-1 border-l border-vsc-border ml-1.5 pl-2.5 pt-1">
          {fields.map((field) => (
            <div key={field.name} className="flex flex-col gap-0.5">
              <FieldLabel name={field.name} schema={field.schema} />
              <FieldInput
                schema={field.schema}
                value={obj[field.name]}
                onChange={(v) => setField(field.name, v)}
                depth={depth + 1}
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
> = ({ schema, value, onChange, depth }) => {
  const lookup = useContext(TypeLookupContext);
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
            />
          </div>
          <Button
            variant="ghost"
            size="icon-xs"
            className="text-vsc-red shrink-0"
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
        onClick={() =>
          onChange([...items, defaultValueForSchema(schema.item, lookup)])
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
  onRename: (next: string) => void;
}> = ({ mapKey, siblingKeys, onRename }) => {
  const [draft, setDraft] = useDraft(mapKey);
  const collides = draft !== mapKey && siblingKeys.includes(draft);
  return (
    <Input
      className="h-7 text-xs font-vsc-mono w-[130px] shrink-0"
      value={draft}
      placeholder="key"
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
> = ({ schema, value, onChange, depth }) => {
  const lookup = useContext(TypeLookupContext);
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
            onRename={(nk) => rebuild((e, j) => (j === i ? [nk, e[1]] : e))}
          />
          <div className="flex-1 min-w-0">
            <FieldInput
              schema={schema.value}
              value={v}
              onChange={(nv) => rebuild((e, j) => (j === i ? [e[0], nv] : e))}
              depth={depth + 1}
            />
          </div>
          <Button
            variant="ghost"
            size="icon-xs"
            className="text-vsc-red shrink-0"
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
        onClick={() =>
          onChange({
            ...obj,
            [freshKey()]: defaultValueForSchema(schema.value, lookup),
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
> = ({ schema, value, onChange, depth, refPath, placeholder }) => {
  const lookup = useContext(TypeLookupContext);
  const isSet = value !== null && value !== undefined;
  return (
    <div className="flex flex-col gap-1">
      <label className="flex items-center gap-1.5 text-[10px] text-vsc-description">
        <Switch
          checked={isSet}
          onCheckedChange={(on) =>
            onChange(on ? defaultValueForSchema(schema.inner, lookup) : null)
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
          refPath={refPath}
          placeholder={placeholder}
        />
      )}
    </div>
  );
};

const UnionField: FC<
  FieldInputProps & { schema: Extract<FieldSchema, { type: 'union' }> }
> = ({ schema, value, onChange, depth, refPath, placeholder }) => {
  const lookup = useContext(TypeLookupContext);
  const detected = activeUnionVariant(value, schema.variants, lookup);
  const [chosen, setChosen] = useState(0);
  // The explicit choice wins as long as the value inhabits it: first-match
  // detection alone would snap `float` back to `int` (0 matches int first)
  // and make overlapping variants unreachable.
  const chosenSchema = schema.variants[chosen];
  const active =
    detected === -1
      ? chosen
      : chosenSchema !== undefined &&
          valueMatchesSchema(value, chosenSchema, lookup)
        ? chosen
        : detected;
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
          const index = Number(v);
          setChosen(index);
          onChange(defaultValueForSchema(schema.variants[index], lookup));
        }}
      />
      {schema.variants[active] && (
        <FieldInput
          schema={schema.variants[active]}
          value={value}
          onChange={onChange}
          depth={depth}
          refPath={refPath}
          placeholder={placeholder}
        />
      )}
    </div>
  );
};

/** Fallback editor for nodes without a typed widget: a JSON textarea that
 *  commits on every parseable edit. Like NumberField, an empty or unparseable
 *  draft is an error state, not a deletion — the last committed value stays
 *  in place (so e.g. emptying a map value doesn't delete the row) and the
 *  invalid style flags the draft. */
const RawJsonField: FC<FieldInputProps> = ({
  schema,
  value,
  onChange,
  disabled,
  placeholder,
}) => {
  const canonical = value === undefined ? '' : JSON.stringify(value);
  const [draft, setDraft] = useDraft(canonical);
  const parse = (text: string): { ok: true; value: unknown } | { ok: false } => {
    if (text.trim() === '') return { ok: false };
    try {
      return { ok: true, value: JSON.parse(text) };
    } catch {
      return { ok: false };
    }
  };
  return (
    <Textarea
      className="min-h-[28px] px-2 py-1 font-vsc-mono text-xs resize-y"
      rows={1}
      value={draft}
      placeholder={placeholder ?? `JSON (${schemaLabel(schema)})`}
      aria-invalid={!disabled && !parse(draft).ok}
      onChange={(e) => {
        setDraft(e.target.value);
        const parsed = parse(e.target.value);
        if (parsed.ok) onChange(parsed.value);
      }}
    />
  );
};
