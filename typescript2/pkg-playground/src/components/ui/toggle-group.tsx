import * as React from 'react';
import { cn } from '../../lib/utils';

interface ToggleGroupProps<T extends string> {
  value: T;
  onValueChange: (value: T) => void;
  options: { value: T; label: React.ReactNode }[];
  className?: string;
  size?: 'sm' | 'default';
}

function ToggleGroup<T extends string>({
  value,
  onValueChange,
  options,
  className,
  size = 'default',
}: ToggleGroupProps<T>) {
  return (
    <div className={cn('inline-flex items-center gap-0.5', className)}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          onClick={() => onValueChange(option.value)}
          className={cn(
            'rounded font-vsc-mono cursor-pointer transition-colors',
            size === 'sm' ? 'px-1.5 py-0.5 text-[10px]' : 'px-2 py-1 text-xs',
            value === option.value
              ? 'bg-vsc-accent text-vsc-accent-fg'
              : 'text-muted-foreground hover:text-foreground hover:bg-accent',
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export { ToggleGroup };
export type { ToggleGroupProps };
