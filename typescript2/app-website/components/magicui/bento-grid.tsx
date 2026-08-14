'use client';

import type React from 'react';
import { cn } from '@/lib/utils';

interface BentoGridProps extends React.HTMLAttributes<HTMLDivElement> {
  children: React.ReactNode;
}

export function BentoGrid({ children, className, ...props }: BentoGridProps) {
  return (
    <div
      className={cn(
        'grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3',
        className,
      )}
      {...props}
    >
      {children}
    </div>
  );
}
