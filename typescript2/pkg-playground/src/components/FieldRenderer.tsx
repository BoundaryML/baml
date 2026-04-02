import { type FC, useState } from 'react';
import type { FieldType } from '../worker-protocol';
import { Input } from './ui/input';
import { Textarea } from './ui/textarea';
import { ToggleGroup } from './ui/toggle-group';
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from './ui/collapsible';
import { Button } from './ui/button';
import { ChevronRight, Plus, Trash2 } from 'lucide-react';
import { cn } from '../lib/utils';

interface FieldRendererProps {
  fieldType: FieldType;
  value: unknown;
  onChange: (value: unknown) => void;
  depth?: number;
  path?: string;
}

export const FieldRenderer: FC<FieldRendererProps> = ({
  fieldType,
  value,
  onChange,
  depth = 0,
  path = '$',
}) => {
  switch (fieldType.type) {
    case 'string':
      return (
        <Input
          type="text"
          value={typeof value === 'string' ? value : ''}
          onChange={(e) => onChange(e.target.value)}
          className="h-7 font-vsc-mono text-xs"
          placeholder="string"
        />
      );

    case 'int':
      return (
        <Input
          type="number"
          step={1}
          value={typeof value === 'number' ? value : ''}
          onChange={(e) => {
            const v = e.target.value;
            onChange(v === '' ? undefined : parseInt(v, 10));
          }}
          className="h-7 font-vsc-mono text-xs"
          placeholder="0"
        />
      );

    case 'float':
      return (
        <Input
          type="number"
          step="any"
          value={typeof value === 'number' ? value : ''}
          onChange={(e) => {
            const v = e.target.value;
            onChange(v === '' ? undefined : parseFloat(v));
          }}
          className="h-7 font-vsc-mono text-xs"
          placeholder="0.0"
        />
      );

    case 'bool':
      return (
        <label className="flex items-center gap-1.5 cursor-pointer">
          <input
            type="checkbox"
            checked={!!value}
            onChange={(e) => onChange(e.target.checked)}
            className="accent-vsc-accent"
          />
          <span className="font-vsc-mono text-xs text-vsc-text-muted">
            {value ? 'true' : 'false'}
          </span>
        </label>
      );

    case 'null':
      return (
        <span className="font-vsc-mono text-xs text-vsc-text-faint px-1">null</span>
      );

    case 'enum':
      if (fieldType.values.length <= 5) {
        return (
          <ToggleGroup
            value={typeof value === 'string' ? value : ''}
            onValueChange={(v) => onChange(v)}
            options={fieldType.values.map((v) => ({ value: v, label: v }))}
            size="sm"
          />
        );
      }
      return (
        <select
          value={typeof value === 'string' ? value : ''}
          onChange={(e) => onChange(e.target.value)}
          className="h-7 rounded border border-vsc-border bg-vsc-input font-vsc-mono text-xs text-vsc-text px-1"
        >
          <option value="">Select {fieldType.name}...</option>
          {fieldType.values.map((v) => (
            <option key={v} value={v}>{v}</option>
          ))}
        </select>
      );

    case 'optional':
      return (
        <OptionalField
          fieldType={fieldType}
          value={value}
          onChange={onChange}
          depth={depth}
          path={path}
        />
      );

    case 'literal':
      return (
        <span className="font-vsc-mono text-xs text-vsc-text-faint px-1">
          {JSON.stringify(fieldType.value.value)}
        </span>
      );

    case 'class':
      // At render depth >= 3, fall back to JSON textarea to prevent UI clutter
      if (depth >= 3) {
        return <JsonFallback value={value} onChange={onChange} />;
      }
      return (
        <ClassField
          fieldType={fieldType}
          value={value}
          onChange={onChange}
          depth={depth}
          path={path}
        />
      );

    case 'list':
      return (
        <ListField
          fieldType={fieldType}
          value={value}
          onChange={onChange}
          depth={depth}
          path={path}
        />
      );

    case 'map':
      return (
        <MapField
          fieldType={fieldType}
          value={value}
          onChange={onChange}
          depth={depth}
          path={path}
        />
      );

    case 'union':
      return <JsonFallback value={value} onChange={onChange} />;

    case 'recursiveRef':
      return (
        <div className="space-y-0.5">
          <span className="font-vsc-mono text-[10px] text-vsc-text-faint">
            {fieldType.name} (JSON)
          </span>
          <JsonFallback value={value} onChange={onChange} />
        </div>
      );

    case 'any':
    case 'media':
    default:
      return <JsonFallback value={value} onChange={onChange} />;
  }
};

/** Optional field — toggle switch to enable/disable + renders inner type when enabled. */
const OptionalField: FC<
  FieldRendererProps & { fieldType: Extract<FieldType, { type: 'optional' }> }
> = ({ fieldType, value, onChange, depth, path }) => {
  const isEnabled = value !== undefined && value !== null;
  return (
    <div className="space-y-1">
      <label className="flex items-center gap-1.5 cursor-pointer">
        <input
          type="checkbox"
          checked={isEnabled}
          onChange={(e) => {
            if (!e.target.checked) {
              onChange(undefined);
            } else {
              onChange(getDefaultValue(fieldType.inner));
            }
          }}
          className="accent-vsc-accent"
        />
        <span className="font-vsc-mono text-[10px] text-vsc-text-faint">optional</span>
      </label>
      {isEnabled && (
        <FieldRenderer
          fieldType={fieldType.inner}
          value={value}
          onChange={onChange}
          depth={depth}
          path={path}
        />
      )}
    </div>
  );
};

/** Raw JSON textarea fallback for unsupported types. */
const JsonFallback: FC<{ value: unknown; onChange: (value: unknown) => void }> = ({
  value,
  onChange,
}) => {
  const [text, setText] = useState(() =>
    value !== undefined ? JSON.stringify(value, null, 2) : '',
  );
  return (
    <Textarea
      value={text}
      onChange={(e) => {
        setText(e.target.value);
        try {
          onChange(JSON.parse(e.target.value));
        } catch {
          // Don't update until valid JSON
        }
      }}
      className="font-vsc-mono text-xs min-h-[56px]"
      placeholder="JSON"
    />
  );
};

/** Collapsible nested form for a class type. */
const ClassField: FC<
  FieldRendererProps & { fieldType: Extract<FieldType, { type: 'class' }> }
> = ({ fieldType, value, onChange, depth, path }) => {
  const [open, setOpen] = useState((depth ?? 0) < 2);
  const obj =
    typeof value === 'object' && value !== null && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {};

  const handleFieldChange = (fieldName: string, fieldValue: unknown) => {
    onChange({ ...obj, [fieldName]: fieldValue });
  };

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger className="flex items-center gap-1 cursor-pointer text-vsc-text-muted hover:text-vsc-text">
        <ChevronRight size={12} className={cn('transition-transform', open && 'rotate-90')} />
        <span className="font-vsc-mono text-[11px]">{fieldType.name}</span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="space-y-2 pl-3 border-l border-vsc-border mt-1">
          {fieldType.fields.map((field) => (
            <div key={field.name} className="space-y-0.5">
              <label className="block font-vsc-mono text-[11px] text-vsc-text-muted">
                {field.name}
              </label>
              <FieldRenderer
                fieldType={field.fieldType}
                value={obj[field.name]}
                onChange={(v) => handleFieldChange(field.name, v)}
                depth={(depth ?? 0) + 1}
                path={`${path}.${field.name}`}
              />
            </div>
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
};

/** Dynamic list with add/remove per-item. */
const ListField: FC<
  FieldRendererProps & { fieldType: Extract<FieldType, { type: 'list' }> }
> = ({ fieldType, value, onChange, depth, path }) => {
  const items = Array.isArray(value) ? value : [];

  return (
    <div className="space-y-1">
      {items.map((item, i) => (
        <div key={i} className="flex items-start gap-1">
          <div className="flex-1">
            <FieldRenderer
              fieldType={fieldType.item}
              value={item}
              onChange={(v) => {
                const next = [...items];
                next[i] = v;
                onChange(next);
              }}
              depth={(depth ?? 0) + 1}
              path={`${path}[${i}]`}
            />
          </div>
          <Button
            variant="ghost"
            size="icon-xs"
            className="shrink-0 mt-0.5"
            onClick={() => onChange(items.filter((_, j) => j !== i))}
          >
            <Trash2 size={12} />
          </Button>
        </div>
      ))}
      <Button
        variant="ghost"
        size="xs"
        className="h-6 text-[10px]"
        onClick={() => onChange([...items, getDefaultValue(fieldType.item)])}
      >
        <Plus size={12} /> Add item
      </Button>
    </div>
  );
};

/** Key-value map with add/remove entries. */
const MapField: FC<
  FieldRendererProps & { fieldType: Extract<FieldType, { type: 'map' }> }
> = ({ fieldType, value, onChange, depth, path }) => {
  const obj =
    typeof value === 'object' && value !== null && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {};
  const entries = Object.entries(obj);

  return (
    <div className="space-y-1">
      {entries.map(([key, val]) => (
        <div key={key} className="flex items-start gap-1">
          <Input
            type="text"
            value={key}
            onChange={(e) => {
              const newKey = e.target.value;
              const next: Record<string, unknown> = {};
              for (const [k, v] of entries) {
                next[k === key ? newKey : k] = v;
              }
              onChange(next);
            }}
            className="h-7 w-24 shrink-0 font-vsc-mono text-xs"
            placeholder="key"
          />
          <div className="flex-1">
            <FieldRenderer
              fieldType={fieldType.value}
              value={val}
              onChange={(v) => onChange({ ...obj, [key]: v })}
              depth={(depth ?? 0) + 1}
              path={`${path}[${key}]`}
            />
          </div>
          <Button
            variant="ghost"
            size="icon-xs"
            className="shrink-0 mt-0.5"
            onClick={() => {
              const { [key]: _, ...rest } = obj;
              onChange(rest);
            }}
          >
            <Trash2 size={12} />
          </Button>
        </div>
      ))}
      <Button
        variant="ghost"
        size="xs"
        className="h-6 text-[10px]"
        onClick={() => {
          let newKey = `key${entries.length}`;
          while (newKey in obj) newKey += '_';
          onChange({ ...obj, [newKey]: getDefaultValue(fieldType.value) });
        }}
      >
        <Plus size={12} /> Add entry
      </Button>
    </div>
  );
};

/** Produce a sensible default value for a field type. */
export function getDefaultValue(fieldType: FieldType): unknown {
  switch (fieldType.type) {
    case 'string':
      return '';
    case 'int':
      return 0;
    case 'float':
      return 0.0;
    case 'bool':
      return false;
    case 'null':
      return null;
    case 'enum':
      return fieldType.values[0] ?? '';
    case 'optional':
      return undefined;
    case 'literal':
      return fieldType.value.value;
    case 'class': {
      const obj: Record<string, unknown> = {};
      for (const f of fieldType.fields) {
        obj[f.name] = getDefaultValue(f.fieldType);
      }
      return obj;
    }
    case 'list':
      return [];
    case 'map':
      return {};
    case 'union':
      return fieldType.variants.length > 0 ? getDefaultValue(fieldType.variants[0]) : null;
    default:
      return null;
  }
}
