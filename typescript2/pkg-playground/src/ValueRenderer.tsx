/**
 * Shared value renderer for Playground output.
 *
 * Recursively walks objects/arrays so nested $baml-typed values (e.g. media
 * inside a class) are rendered with their registered component.
 * The displayMode prop controls rendering context (inline, expanded, auto).
 */

import { useState, type FC } from 'react';
import { ChevronRight } from 'lucide-react';
import { CopyButton } from './components/CopyButton';
import { CodeBlock } from './components/ui/code-block';
import { getBamlType, getResultRenderer, BAML_TYPE_KEY } from './result-renderers';
import type { ResultRendererProps, DisplayMode } from './result-renderers';

function resolve(
  type: string,
  customRenderers?: Record<string, FC<ResultRendererProps>>,
): FC<ResultRendererProps> | undefined {
  return customRenderers?.[type] ?? getResultRenderer(type);
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

export const ValueRenderer: FC<{
  value: unknown;
  customRenderers?: Record<string, FC<ResultRendererProps>>;
  depth?: number;
  path?: string;
  displayMode?: DisplayMode;
}> = ({ value, customRenderers, depth = 0, path = '$', displayMode = 'auto' }) => {
  const isInline = displayMode === 'inline';
  const [collapsed, setCollapsed] = useState(isInline || depth >= 2);

  // Primitives with type coloring (same for all modes)
  if (value == null) return <span className="font-vsc-mono text-xs text-vsc-text-faint">null</span>;
  if (typeof value === 'string') return <span className="font-vsc-mono text-xs text-green-400">"{value}"</span>;
  if (typeof value === 'number') return <span className="font-vsc-mono text-xs text-cyan-400">{value}</span>;
  if (typeof value === 'bigint') return <span className="font-vsc-mono text-xs text-cyan-400">{`${value}n`}</span>;
  if (typeof value === 'boolean') return <span className="font-vsc-mono text-xs text-yellow-400">{String(value)}</span>;
  if (typeof value !== 'object') return <span className="font-vsc-mono text-xs text-vsc-text">{stringifyValue(value)}</span>;

  // $baml.type dispatch
  const type = getBamlType(value);
  if (type) {
    const Renderer = resolve(type, customRenderers);
    if (Renderer) return <Renderer value={value} displayMode={displayMode} />;
    // Fall through to object rendering so class fields are visible
    // (the className prefix is added at line ~108 below)
  }

  // $type dispatch — BAML instance types from bex_value_to_json
  const dollarType = (value as Record<string, unknown>).$type;
  if (typeof dollarType === 'string') {
    const Renderer = resolve(dollarType, customRenderers);
    if (Renderer) return <Renderer value={value} displayMode={displayMode} />;
  }

  // Array
  if (Array.isArray(value)) {
    if (value.length === 0) return <span className="font-vsc-mono text-xs text-vsc-text-faint">[]</span>;

    // Inline mode: render items on a single line
    if (isInline) {
      return (
        <span className="font-vsc-mono text-xs text-vsc-text-faint">
          {'['}
          {value.map((item, i) => (
            <span key={i}>
              {i > 0 && ', '}
              <ValueRenderer value={item} customRenderers={customRenderers} depth={depth + 1} displayMode="inline" />
            </span>
          ))}
          {']'}
        </span>
      );
    }

    const showToggle = value.length > 3;
    return (
      <div className="group/node">
        <div className="flex items-center gap-0.5">
          {showToggle && (
            <button onClick={() => setCollapsed(!collapsed)} className="p-0 text-vsc-text-muted hover:text-vsc-text">
              <ChevronRight size={12} className={`transition-transform ${collapsed ? '' : 'rotate-90'}`} />
            </button>
          )}
          <span className="font-vsc-mono text-xs text-vsc-text-faint">[{value.length}]</span>
          <CopyButton text={stringifyValue(value, 2)} className="opacity-0 group-hover/node:opacity-100" iconSize={11} />
        </div>
        {!collapsed && (
          <div className="space-y-1 pl-3 border-l border-vsc-border-subtle mt-0.5">
            {value.map((item, i) => (
              <ValueRenderer key={i} value={item} customRenderers={customRenderers} depth={depth + 1} path={`${path}[${i}]`} displayMode={displayMode} />
            ))}
          </div>
        )}
      </div>
    );
  }

  // Plain object
  const entries = Object.entries(value as Record<string, unknown>).filter(([k]) => k !== BAML_TYPE_KEY);
  if (entries.length === 0) return <span className="font-vsc-mono text-xs text-vsc-text-faint">{'{}'}</span>;

  // Inline mode: render as a single-line summary
  if (isInline) {
    const className = (value as Record<string, unknown>).$baml != null
      ? getBamlType(value) ?? undefined
      : undefined;
    const prefix = className ? `${className} ` : '';
    return (
      <span className="font-vsc-mono text-xs text-vsc-text">
        {prefix}{'{ '}
        {entries.map(([key, val], i) => (
          <span key={key}>
            {i > 0 && ', '}
            <span className="text-vsc-text-muted">{key}</span>
            {': '}
            <ValueRenderer value={val} customRenderers={customRenderers} depth={depth + 1} displayMode="inline" />
          </span>
        ))}
        {' }'}
      </span>
    );
  }

  const showToggle = entries.length > 3;
  return (
    <div className="group/node">
      <div className="flex items-center gap-0.5">
        {showToggle && (
          <button onClick={() => setCollapsed(!collapsed)} className="p-0 text-vsc-text-muted hover:text-vsc-text">
            <ChevronRight size={12} className={`transition-transform ${collapsed ? '' : 'rotate-90'}`} />
          </button>
        )}
        <span className="font-vsc-mono text-xs text-vsc-text-faint">{'{'}…{'}'} {entries.length} keys</span>
        <CopyButton text={stringifyValue(value, 2)} className="opacity-0 group-hover/node:opacity-100" iconSize={11} />
      </div>
      {!collapsed && (
        <div className="space-y-1 pl-3 mt-0.5">
          {entries.map(([key, val]) => {
            const isComplex = val != null && typeof val === 'object';
            if (!isComplex) {
              return (
                <div key={key} className="flex gap-1.5 items-baseline font-vsc-mono text-xs">
                  <span className="text-vsc-text-muted shrink-0">{key}:</span>
                  <ValueRenderer value={val} customRenderers={customRenderers} depth={depth + 1} path={`${path}.${key}`} displayMode={displayMode} />
                </div>
              );
            }
            return (
              <div key={key} className="space-y-0.5">
                <div className="font-vsc-mono text-xs text-vsc-text-muted">{key}:</div>
                <div className="pl-2">
                  <ValueRenderer value={val} customRenderers={customRenderers} depth={depth + 1} path={`${path}.${key}`} displayMode={displayMode} />
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
