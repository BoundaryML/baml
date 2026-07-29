/**
 * Shared value renderer for Playground output.
 *
 * Recursively walks objects/arrays so nested $baml-typed values (e.g. media
 * inside a class) are rendered with their registered component.
 * The displayMode prop controls rendering context (inline, expanded, auto).
 */

import { useState, type FC, type ReactNode } from 'react';
import { ChevronRight } from 'lucide-react';
import { CopyButton } from './components/CopyButton';
import {
  getBamlType,
  getResultRenderer,
  BAML_TYPE_KEY,
} from './result-renderers';
import type { ResultRendererProps, DisplayMode } from './result-renderers';

interface ValueRendererProps {
  value: unknown;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
  depth?: number;
  path?: string;
  displayMode?: DisplayMode;
}

interface TreeNodeProps extends ValueRendererProps {
  keyName?: string | number;
}

function resolve(
  type: string,
  customRenderers?: Record<string, FC<ResultRendererProps>>,
): FC<ResultRendererProps> | undefined {
  return customRenderers?.[type] ?? getResultRenderer(type);
}

function rendererFor(
  value: unknown,
  customRenderers?: Record<string, FC<ResultRendererProps>>,
): FC<ResultRendererProps> | undefined {
  const bamlType = getBamlType(value);
  if (bamlType) return resolve(bamlType, customRenderers);

  if (value != null && typeof value === 'object') {
    const dollarType = (value as Record<string, unknown>).$type;
    if (typeof dollarType === 'string') {
      return resolve(dollarType, customRenderers);
    }
  }

  return undefined;
}

function stringifyValue(value: unknown, space?: number): string {
  if (typeof value === 'bigint') return value.toString();
  try {
    const json = JSON.stringify(
      value,
      (_, nested) => (typeof nested === 'bigint' ? nested.toString() : nested),
      space,
    );
    return json ?? String(value);
  } catch {
    return String(value);
  }
}

const PrimitiveValue: FC<{ value: unknown }> = ({ value }) => {
  if (value == null) {
    return <span className="text-vsc-text-faint">null</span>;
  }
  if (typeof value === 'string') {
    return <span className="text-green-400">{JSON.stringify(value)}</span>;
  }
  if (typeof value === 'number') {
    return <span className="text-cyan-400">{String(value)}</span>;
  }
  if (typeof value === 'bigint') {
    return <span className="text-cyan-400">{`${value}n`}</span>;
  }
  if (typeof value === 'boolean') {
    return <span className="text-yellow-400">{String(value)}</span>;
  }
  return <span className="text-vsc-text">{stringifyValue(value)}</span>;
};

const KeyName: FC<{ value?: string | number }> = ({ value }) => {
  if (value == null) return null;
  return (
    <>
      <span className="shrink-0 text-vsc-text-muted">
        {typeof value === 'string' ? JSON.stringify(value) : value}
      </span>
      <span className="mr-1 text-vsc-text-faint">:</span>
    </>
  );
};

const TreeRow: FC<{
  keyName?: string | number;
  children: ReactNode;
}> = ({ keyName, children }) => (
  <div className="flex min-w-0 items-start py-0.5 font-vsc-mono text-xs leading-4">
    <span className="h-4 w-4 shrink-0" aria-hidden="true" />
    <KeyName value={keyName} />
    <div className="min-w-0 flex-1">{children}</div>
  </div>
);

const InlineValue: FC<ValueRendererProps> = ({
  value,
  customRenderers,
  depth = 0,
}) => {
  if (value == null || typeof value !== 'object') {
    return <PrimitiveValue value={value} />;
  }

  const Renderer = rendererFor(value, customRenderers);
  if (Renderer) return <Renderer value={value} displayMode="inline" />;

  if (Array.isArray(value)) {
    return (
      <span className="text-vsc-text-faint">
        {'['}
        {value.map((item, index) => (
          <span key={index}>
            {index > 0 && ', '}
            <InlineValue
              value={item}
              customRenderers={customRenderers}
              depth={depth + 1}
            />
          </span>
        ))}
        {']'}
      </span>
    );
  }

  const entries = Object.entries(value as Record<string, unknown>).filter(
    ([key]) => key !== BAML_TYPE_KEY,
  );
  const className =
    (value as Record<string, unknown>).$baml != null
      ? (getBamlType(value) ?? undefined)
      : undefined;

  return (
    <span className="text-vsc-text">
      {className && `${className} `}
      {'{ '}
      {entries.map(([key, nested], index) => (
        <span key={key}>
          {index > 0 && ', '}
          <span className="text-vsc-text-muted">{JSON.stringify(key)}</span>
          {': '}
          <InlineValue
            value={nested}
            customRenderers={customRenderers}
            depth={depth + 1}
          />
        </span>
      ))}
      {' }'}
    </span>
  );
};

const TreeNode: FC<TreeNodeProps> = ({
  value,
  customRenderers,
  depth = 0,
  path = '$',
  displayMode = 'auto',
  keyName,
}) => {
  const [collapsed, setCollapsed] = useState(depth >= 2);

  if (value == null || typeof value !== 'object') {
    return (
      <TreeRow keyName={keyName}>
        <PrimitiveValue value={value} />
      </TreeRow>
    );
  }

  const Renderer = rendererFor(value, customRenderers);
  if (Renderer) {
    return (
      <TreeRow keyName={keyName}>
        <Renderer value={value} displayMode={displayMode} />
      </TreeRow>
    );
  }

  const isArray = Array.isArray(value);
  const entries: [string | number, unknown][] = isArray
    ? value.map((nested, index) => [index, nested])
    : Object.entries(value as Record<string, unknown>).filter(
        ([key]) => key !== BAML_TYPE_KEY,
      );
  const open = isArray ? '[' : '{';
  const close = isArray ? ']' : '}';
  const kind = isArray ? 'array' : 'object';

  if (entries.length === 0) {
    return (
      <TreeRow keyName={keyName}>
        <span className="text-vsc-text-faint">
          {open}
          {close}
        </span>
      </TreeRow>
    );
  }

  return (
    <div className="group/node min-w-0 font-vsc-mono text-xs">
      <div className="flex min-w-0 items-start py-0.5 leading-4 hover:bg-vsc-surface">
        <button
          type="button"
          aria-expanded={!collapsed}
          aria-label={`${collapsed ? 'Expand' : 'Collapse'} ${kind}`}
          onClick={() => setCollapsed((current) => !current)}
          className="flex h-4 w-4 shrink-0 items-center justify-center p-0 text-vsc-text-muted hover:text-vsc-text"
        >
          <ChevronRight
            size={12}
            className={`transition-transform ${collapsed ? '' : 'rotate-90'}`}
          />
        </button>
        <KeyName value={keyName} />
        <span className="text-vsc-text-faint">{open}</span>
        {collapsed && (
          <span className="text-vsc-text-faint">
            …{close}
          </span>
        )}
        <CopyButton
          text={stringifyValue(value, 2)}
          className="-my-1.5 ml-0.5 h-7 w-7 opacity-0 group-hover/node:opacity-100"
          iconSize={11}
        />
      </div>
      {!collapsed && (
        <>
          <div className="ml-4 border-l border-vsc-border-subtle pl-1">
            {entries.map(([childKey, nested]) => (
              <TreeNode
                key={childKey}
                keyName={childKey}
                value={nested}
                customRenderers={customRenderers}
                depth={depth + 1}
                path={
                  isArray
                    ? `${path}[${childKey}]`
                    : `${path}.${String(childKey)}`
                }
                displayMode={displayMode}
              />
            ))}
          </div>
          <div className="py-0.5 pl-4 font-vsc-mono text-xs leading-4 text-vsc-text-faint">
            {close}
          </div>
        </>
      )}
    </div>
  );
};

export const ValueRenderer: FC<ValueRendererProps> = ({
  value,
  customRenderers,
  depth = 0,
  path = '$',
  displayMode = 'auto',
}) => {
  if (displayMode === 'inline') {
    return (
      <span className="font-vsc-mono text-xs">
        <InlineValue
          value={value}
          customRenderers={customRenderers}
          depth={depth}
        />
      </span>
    );
  }

  return (
    <TreeNode
      value={value}
      customRenderers={customRenderers}
      depth={depth}
      path={path}
      displayMode={displayMode}
    />
  );
};
