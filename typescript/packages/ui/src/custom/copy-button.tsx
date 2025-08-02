'use client';

import { type VariantProps, cva } from 'class-variance-authority';
import type React from 'react';
import { useState } from 'react';
import { Button } from '../components/button';
import { toast } from '../components/sonner';
import { cn } from '../lib/utils';
import { Icons } from './icons';

export type CopyState = 'idle' | 'copying' | 'copied';

const copyButtonVariants = cva('relative inline-flex items-center', {
  defaultVariants: {
    size: 'default',
    variant: 'ghost',
  },
  variants: {
    size: {
      sm: 'h-7 px-2',
      default: 'h-9 px-3',
      lg: 'h-10 px-4',
    },
    variant: {
      ghost: 'hover:bg-accent hover:text-accent-foreground',
      outline:
        'border border-input bg-transparent hover:bg-accent hover:text-accent-foreground',
      secondary: 'bg-secondary text-secondary-foreground hover:bg-secondary/80',
      default: 'bg-primary text-primary-foreground hover:bg-primary/90',
    },
  },
});

export interface CopyButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof copyButtonVariants> {
  text: string;
  onCopied?: () => void;
  showToast?: boolean;
  successMessage?: string;
  errorMessage?: string;
}

export function CopyButton({
  className,
  text,
  onCopied,
  size,
  variant,
  showToast = true,
  children,
  successMessage = 'Copied to clipboard',
  errorMessage = 'Failed to copy to clipboard',
  ...props
}: CopyButtonProps) {
  const [copyState, setCopyState] = useState<CopyState>('idle');

  const getIconSize = () => {
    switch (size) {
      case 'sm':
        return 'xs';
      case 'lg':
        return 'lg';
      default:
        return 'sm';
    }
  };

  const renderIcon = () => {
    const iconSize = getIconSize();

    switch (copyState) {
      case 'copying':
        return <Icons.Spinner size={iconSize} className="animate-spin" />;
      case 'copied':
        return <Icons.Check size={iconSize} />;
      default:
        return <Icons.Copy size={iconSize} />;
    }
  };

  const copyToClipboard = async (
    event: React.MouseEvent<HTMLButtonElement>,
  ) => {
    event.stopPropagation();

    setCopyState('copying');
    try {
      await navigator.clipboard.writeText(text);
      setCopyState('copied');
      if (showToast) {
        toast.success(successMessage);
      }
      onCopied?.();
      setTimeout(() => setCopyState('idle'), 2000);
    } catch (err) {
      console.error('Failed to copy:', err);
      if (showToast) {
        toast.error(errorMessage);
      }
      setCopyState('idle');
    }
  };

  return (
    <Button
      type="button"
      className={cn(copyButtonVariants({ size, variant }), className)}
      onClick={copyToClipboard}
      aria-label={copyState === 'copied' ? 'Copied' : 'Copy to clipboard'}
      {...props}
    >
      {renderIcon()}
      {children && <span className="ml-2">{children}</span>}
    </Button>
  );
}
