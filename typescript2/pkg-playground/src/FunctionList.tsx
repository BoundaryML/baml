'use client';

import { useWasmReady, useWasmError, useFunctions } from './BamlPlaygroundProvider';

interface FunctionListProps {
  /** Called when a function is selected */
  onSelect?: (functionName: string) => void;
  /** Currently selected function */
  selected?: string;
  /** Custom class name */
  className?: string;
}

/**
 * Displays a list of functions defined in the BAML project.
 */
export function FunctionList({ onSelect, selected, className }: FunctionListProps) {
  const isReady = useWasmReady();
  const error = useWasmError();
  const functions = useFunctions();

  if (error) {
    return (
      <div className={className} style={{ color: 'red' }}>
        Error: {error}
      </div>
    );
  }

  if (!isReady) {
    return (
      <div className={className} style={{ color: 'gray' }}>
        Loading BAML...
      </div>
    );
  }

  if (functions.length === 0) {
    return (
      <div className={className} style={{ color: 'gray' }}>
        No functions defined
      </div>
    );
  }

  return (
    <ul className={className} style={{ listStyle: 'none', padding: 0, margin: 0 }}>
      {functions.map((fn) => (
        <li
          key={fn}
          onClick={() => onSelect?.(fn)}
          style={{
            padding: '8px 12px',
            cursor: onSelect ? 'pointer' : 'default',
            backgroundColor: selected === fn ? '#e0e0e0' : 'transparent',
            borderRadius: '4px',
          }}
        >
          {fn}
        </li>
      ))}
    </ul>
  );
}
