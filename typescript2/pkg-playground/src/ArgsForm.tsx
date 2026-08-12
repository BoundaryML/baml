// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing public component filename.
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

import { ChevronRight, Plus, SquarePen, Trash2 } from 'lucide-react';
import {
  createContext,
  type FC,
  type ReactNode,
  useContext,
  useMemo,
  useState,
} from 'react';

import {
  activeUnionVariant,
  defaultValueForSchema,
  enumValue,
  enumVariantOf,
  isPlainObject,
  isRawJsonSchema,
  resolveRef,
  schemaLabel,
  type TypeLookup,
  typeLookupFrom,
  valueMatchesSchema,
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
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from './components/ui/tooltip';
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

/** Whether the rendered widget exposes a native text placeholder. */
function rendersDefaultPlaceholder(
  schema: FieldSchema,
  lookup: TypeLookup,
  refPath: readonly string[] = [],
): boolean {
  if (isRawJsonSchema(schema, lookup)) return true;
  switch (schema.type) {
    case 'string':
    case 'int':
    case 'bigint':
    case 'float':
      return true;
    case 'ref': {
      if (refPath.includes(schema.name)) return true;
      const resolved = resolveRef(schema.name, lookup);
      return resolved?.kind === 'schema'
        ? rendersDefaultPlaceholder(resolved.schema, lookup, [
            ...refPath,
            schema.name,
          ])
        : false;
    }
    case 'union':
      return (
        schema.variants[0] !== undefined &&
        rendersDefaultPlaceholder(schema.variants[0], lookup, refPath)
      );
    default:
      return false;
  }
}

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
      <div className="grid grid-cols-[minmax(8rem,max-content)_minmax(0,1fr)] items-start gap-x-2 gap-y-1.5">
        {params.map((param) => (
          <ParamRow
            key={param.name}
            onChange={(v) => onChange({ ...value, [param.name]: v })}
            onOmit={() => {
              const { [param.name]: _omitted, ...rest } = value;
              onChange(rest);
            }}
            param={param}
            present={param.name in value}
            value={value[param.name]}
          />
        ))}
      </div>
    </TypeLookupContext.Provider>
  );
};

/** Shared field header: name plus a faint type label. */
const FieldLabel: FC<{
  name: string;
  schema: FieldSchema;
  extra?: ReactNode;
}> = ({ name, schema, extra }) => (
  <div className="flex min-h-7 items-center gap-1.5">
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
  const showDefaultHint =
    omitted &&
    param.defaultExpression !== undefined &&
    !rendersDefaultPlaceholder(param.schema, lookup);
  return (
    <div className="contents">
      <FieldLabel
        extra={
          param.hasDefault && (
            <div className="ml-auto flex items-center gap-1.5">
              <TooltipProvider delayDuration={300}>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <input
                      aria-label={`Use an explicit value for ${param.name} instead of its default`}
                      checked={!omitted}
                      className="relative size-3.5 shrink-0 cursor-pointer appearance-none rounded-[3px] border border-vsc-description bg-background after:absolute after:inset-0 after:hidden after:place-items-center after:text-[10px] after:font-bold after:leading-none after:text-vsc-accent-fg after:content-['✓'] checked:border-vsc-accent checked:bg-vsc-accent checked:after:grid focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-vsc-accent/50"
                      onChange={(event) =>
                        event.currentTarget.checked
                          ? onChange(
                              defaultValueForSchema(param.schema, lookup),
                            )
                          : onOmit()
                      }
                      type="checkbox"
                    />
                  </TooltipTrigger>
                  <TooltipContent className="max-w-72" side="top">
                    Checked: send an explicitly provided value. Unchecked: use
                    the argument&apos;s default value.
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
              <SquarePen
                aria-hidden="true"
                className="size-3.5 shrink-0 text-vsc-description"
              />
            </div>
          )
        }
        name={param.name}
        schema={param.schema}
      />
      <fieldset
        className="m-0 min-w-0 border-0 p-0 disabled:opacity-60"
        disabled={omitted}
      >
        <FieldInput
          depth={0}
          disabled={omitted}
          onChange={onChange}
          placeholder={
            param.hasDefault
              ? omitted
                ? param.defaultExpression
                : ''
              : undefined
          }
          schema={param.schema}
          value={value}
        />
        {showDefaultHint && (
          <div className="mt-0.5 font-vsc-mono text-[10px] text-vsc-description">
            default: {param.defaultExpression}
          </div>
        )}
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
      return (
        <span className="font-vsc-mono text-xs text-vsc-text-faint">null</span>
      );
    case 'literal':
      return (
        <span className="font-vsc-mono text-xs text-vsc-description">
          {JSON.stringify(schema.value)}
        </span>
      );
    case 'enumVariant':
      return (
        <EnumField {...props} enumName={schema.name} values={[schema.value]} />
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
            refPath={[...refPath, schema.name]}
            schema={resolved.schema}
          />
        );
      }
      return (
        <ClassSection
          {...props}
          fields={resolved.fields}
          typeName={resolved.name}
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
    onChange={(e) => onChange(e.target.value)}
    placeholder={placeholder}
    value={typeof value === 'string' ? value : ''}
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
    typeof value === 'number' || typeof value === 'bigint' ? String(value) : '';
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
      aria-invalid={!disabled && parse(draft) === null}
      className="h-7 text-xs font-vsc-mono"
      inputMode={integer ? 'numeric' : 'decimal'}
      onChange={(e) => {
        setDraft(e.target.value);
        const num = parse(e.target.value);
        if (num !== null) onChange(num);
      }}
      placeholder={placeholder ?? (integer ? '0' : '0.0')}
      value={draft}
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
        onValueChange={(v) => onChange(enumValue(enumName, v))}
        options={values.map((v) => ({ label: v, value: v }))}
        size="sm"
        value={current ?? ''}
      />
    );
  }
  return (
    <div className="max-w-[240px]">
      <Select
        onChange={(e) => {
          // The empty value is the "select…" placeholder, not a variant.
          if (e.target.value === '') return;
          onChange(enumValue(enumName, e.target.value));
        }}
        value={current ?? ''}
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
    <Collapsible onOpenChange={setOpen} open={open}>
      <CollapsibleTrigger className="flex items-center gap-1 cursor-pointer text-xs text-vsc-description hover:text-foreground">
        <ChevronRight
          className={cn('transition-transform', open && 'rotate-90')}
          size={12}
        />
        <span className="font-vsc-mono">{schemaLabel(schema)}</span>
      </CollapsibleTrigger>
      {/* Closed content is unmounted — nested refs (including back into this
          class) only resolve and render when the user opens the section. */}
      <CollapsibleContent>
        <div className="flex flex-col gap-1 border-l border-vsc-border ml-1.5 pl-2.5 pt-1">
          {fields.map((field) => (
            <div className="flex flex-col gap-0.5" key={field.name}>
              <FieldLabel name={field.name} schema={field.schema} />
              <FieldInput
                depth={depth + 1}
                onChange={(v) => setField(field.name, v)}
                schema={field.schema}
                value={obj[field.name]}
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
        // biome-ignore lint/suspicious/noArrayIndexKey: Argument list values do not have stable identities.
        <div className="flex items-start gap-1" key={i}>
          <div className="flex-1 min-w-0">
            <FieldInput
              depth={depth + 1}
              onChange={(v) =>
                onChange(items.map((cur, j) => (j === i ? v : cur)))
              }
              schema={schema.item}
              value={item}
            />
          </div>
          <Button
            aria-label="Remove item"
            className="text-vsc-red shrink-0"
            onClick={() => onChange(items.filter((_, j) => j !== i))}
            size="icon-xs"
            variant="ghost"
          >
            <Trash2 />
          </Button>
        </div>
      ))}
      <Button
        className="self-start text-vsc-link"
        onClick={() =>
          onChange([...items, defaultValueForSchema(schema.item, lookup)])
        }
        size="xs"
        variant="ghost"
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
      aria-invalid={collides}
      className="h-7 text-xs font-vsc-mono w-[130px] shrink-0"
      onChange={(e) => {
        setDraft(e.target.value);
        if (
          e.target.value !== mapKey &&
          !siblingKeys.includes(e.target.value)
        ) {
          onRename(e.target.value);
        }
      }}
      placeholder="key"
      value={draft}
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
        // biome-ignore lint/suspicious/noArrayIndexKey: Map keys are editable and cannot identify component state.
        <div className="flex items-start gap-1" key={i}>
          <MapKeyInput
            mapKey={k}
            onRename={(nk) => rebuild((e, j) => (j === i ? [nk, e[1]] : e))}
            siblingKeys={entries.map(([sk]) => sk)}
          />
          <div className="flex-1 min-w-0">
            <FieldInput
              depth={depth + 1}
              onChange={(nv) => rebuild((e, j) => (j === i ? [e[0], nv] : e))}
              schema={schema.value}
              value={v}
            />
          </div>
          <Button
            aria-label="Remove entry"
            className="text-vsc-red shrink-0"
            onClick={() => rebuild((e, j) => (j === i ? null : e))}
            size="icon-xs"
            variant="ghost"
          >
            <Trash2 />
          </Button>
        </div>
      ))}
      <Button
        className="self-start text-vsc-link"
        onClick={() =>
          onChange({
            ...obj,
            [freshKey()]: defaultValueForSchema(schema.value, lookup),
          })
        }
        size="xs"
        variant="ghost"
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
      <div className="flex items-center gap-1.5 text-[10px] text-vsc-description">
        <Switch
          checked={isSet}
          onCheckedChange={(on) =>
            onChange(on ? defaultValueForSchema(schema.inner, lookup) : null)
          }
        />
        {isSet ? 'set' : 'null'}
      </div>
      {isSet && (
        <FieldInput
          depth={depth}
          onChange={onChange}
          placeholder={placeholder}
          refPath={refPath}
          schema={schema.inner}
          value={value}
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
        onValueChange={(v) => {
          const index = Number(v);
          setChosen(index);
          onChange(defaultValueForSchema(schema.variants[index], lookup));
        }}
        options={schema.variants.map((v, i) => ({
          label: schemaLabel(v),
          value: String(i),
        }))}
        size="sm"
        value={String(active)}
      />
      {schema.variants[active] && (
        <FieldInput
          depth={depth}
          onChange={onChange}
          placeholder={placeholder}
          refPath={refPath}
          schema={schema.variants[active]}
          value={value}
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
  const parse = (
    text: string,
  ): { ok: true; value: unknown } | { ok: false } => {
    if (text.trim() === '') return { ok: false };
    try {
      return { ok: true, value: JSON.parse(text) };
    } catch {
      return { ok: false };
    }
  };
  return (
    <Textarea
      aria-invalid={!disabled && !parse(draft).ok}
      className="min-h-[28px] px-2 py-1 font-vsc-mono text-xs resize-y"
      onChange={(e) => {
        setDraft(e.target.value);
        const parsed = parse(e.target.value);
        if (parsed.ok) onChange(parsed.value);
      }}
      placeholder={placeholder ?? `JSON (${schemaLabel(schema)})`}
      rows={1}
      value={draft}
    />
  );
};
