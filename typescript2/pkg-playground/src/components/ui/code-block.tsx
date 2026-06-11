import * as React from 'react';
import { cn } from '../../lib/utils';

interface CodeBlockProps extends React.HTMLAttributes<HTMLPreElement> {
  variant?: 'default' | 'error';
}

const CodeBlock = React.forwardRef<HTMLPreElement, CodeBlockProps>(
  ({ variant = 'default', className, ...props }, ref) => {
    return (
      <pre
        ref={ref}
        className={cn(
          'whitespace-pre-wrap break-all font-vsc-mono text-xs leading-relaxed p-2 rounded bg-vsc-bg border border-vsc-border text-vsc-text overflow-auto max-h-[200px] m-0',
          variant === 'error' &&
            'border-vsc-error/20 bg-vsc-error/5 text-vsc-error',
          className,
        )}
        {...props}
      />
    );
  },
);
CodeBlock.displayName = 'CodeBlock';

export { CodeBlock };
