// biome-ignore-all lint/style/useFilenamingConvention: Preserve the existing exported component path.
/**
 * Shared value renderer for Playground output.
 *
 * Recursively walks objects/arrays so nested $baml-typed values (e.g. media
 * inside a class) are rendered with their registered component.
 * The displayMode prop controls rendering context (inline, expanded, auto).
 */

import { ChevronRight } from 'lucide-react';
import { type FC, type ReactNode, useCallback, useState } from 'react';
import { CopyButton } from './components/CopyButton';
import type { DisplayMode, ResultRendererProps } from './result-renderers';
import {
  BAML_TYPE_KEY,
  getBamlType,
  getResultRenderer,
} from './result-renderers';

interface ValueRendererProps {
  value: unknown;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
  depth?: number;
  displayMode?: DisplayMode;
}

interface TreeNodeProps extends ValueRendererProps {
  collapsedByPath: Readonly<Record<string, boolean>>;
  keyName?: string | number;
  onCollapsedChange: (path: string, collapsed: boolean) => void;
  path: string;
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

function visibleEntries(value: Record<string, unknown>): [string, unknown][] {
  return Object.entries(value).filter(
    ([key]) => key !== BAML_TYPE_KEY && key !== '$type',
  );
}

function keyInlineArrayItems(
  values: unknown[],
): { key: string; value: unknown }[] {
  const occurrences = new Map<string, number>();
  return values.map((value) => {
    const serialized = stringifyValue(value);
    const occurrence = occurrences.get(serialized) ?? 0;
    occurrences.set(serialized, occurrence + 1);
    return { key: `${serialized}:${occurrence}`, value };
  });
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
    <span aria-hidden="true" className="h-4 w-4 shrink-0" />
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
  if (Renderer) return <Renderer displayMode="inline" value={value} />;

  if (depth >= 2) {
    return (
      <span className="text-vsc-text-faint">
        {Array.isArray(value) ? '[…]' : '{…}'}
      </span>
    );
  }

  if (Array.isArray(value)) {
    return (
      <span className="text-vsc-text-faint">
        {'['}
        {keyInlineArrayItems(value).map(({ key, value: item }, index) => (
          <span key={key}>
            {index > 0 && ', '}
            <InlineValue
              customRenderers={customRenderers}
              depth={depth + 1}
              value={item}
            />
          </span>
        ))}
        {']'}
      </span>
    );
  }

  const entries = visibleEntries(value as Record<string, unknown>);
  const typeName =
    (value as Record<string, unknown>).$baml != null
      ? (getBamlType(value) ?? undefined)
      : undefined;

  return (
    <span className="text-vsc-text">
      {typeName && `${typeName} `}
      {'{ '}
      {entries.map(([key, nested], index) => (
        <span key={key}>
          {index > 0 && ', '}
          <span className="text-vsc-text-muted">{JSON.stringify(key)}</span>
          {': '}
          <InlineValue
            customRenderers={customRenderers}
            depth={depth + 1}
            value={nested}
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
  displayMode = 'auto',
  collapsedByPath,
  keyName,
  onCollapsedChange,
  path,
}) => {
  const collapsed = collapsedByPath[path] ?? depth >= 2;

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
        <Renderer displayMode={displayMode} value={value} />
      </TreeRow>
    );
  }

  const isArray = Array.isArray(value);
  const entries: [string | number, unknown][] = isArray
    ? value.map((nested, index) => [index, nested])
    : visibleEntries(value as Record<string, unknown>);
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
          aria-expanded={!collapsed}
          aria-label={`${collapsed ? 'Expand' : 'Collapse'} ${kind}${
            keyName == null ? '' : ` ${keyName}`
          }`}
          className="flex h-4 w-4 shrink-0 items-center justify-center p-0 text-vsc-text-muted hover:text-vsc-text"
          onClick={() => onCollapsedChange(path, !collapsed)}
          type="button"
        >
          <ChevronRight
            className={`transition-transform ${collapsed ? '' : 'rotate-90'}`}
            size={12}
          />
        </button>
        <KeyName value={keyName} />
        <span className="text-vsc-text-faint">{open}</span>
        {collapsed && <span className="text-vsc-text-faint">…{close}</span>}
        <CopyButton
          className="-my-1.5 ml-0.5 h-7 w-7 opacity-0 group-hover/node:opacity-100"
          getText={() => stringifyValue(value, 2)}
          iconSize={11}
        />
      </div>
      {!collapsed && (
        <>
          <div className="ml-4 border-l border-vsc-border-subtle pl-1">
            {entries.map(([childKey, nested]) => (
              <TreeNode
                collapsedByPath={collapsedByPath}
                customRenderers={customRenderers}
                depth={depth + 1}
                displayMode={displayMode}
                key={childKey}
                keyName={childKey}
                onCollapsedChange={onCollapsedChange}
                path={`${path}/${JSON.stringify(childKey)}`}
                value={nested}
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
  displayMode = 'auto',
}) => {
  const [collapsedByPath, setCollapsedByPath] = useState<
    Record<string, boolean>
  >({});
  const handleCollapsedChange = useCallback(
    (path: string, collapsed: boolean) => {
      setCollapsedByPath((current) => ({ ...current, [path]: collapsed }));
    },
    [],
  );

  if (displayMode === 'inline') {
    return (
      <span className="font-vsc-mono text-xs">
        <InlineValue
          customRenderers={customRenderers}
          depth={depth}
          value={value}
        />
      </span>
    );
  }

  return (
    <TreeNode
      collapsedByPath={collapsedByPath}
      customRenderers={customRenderers}
      depth={depth}
      displayMode={displayMode}
      onCollapsedChange={handleCollapsedChange}
      path="$"
      value={value}
    />
  );
};
