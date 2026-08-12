'use client';

import type React from 'react';
import { memo } from 'react';

interface AuroraTextProps {
  children: React.ReactNode;
  className?: string;
  colors?: string[];
  speed?: number;
}

export const AuroraText = memo(
  ({
    children,
    className = '',
    colors = ['#FF0080', '#7928CA', '#0070F3', '#38bdf8'],
    speed = 1,
  }: AuroraTextProps) => {
    const gradientStyle = {
      animationDuration: `${10 / speed}s`,
      backgroundImage: `linear-gradient(135deg, ${colors.join(', ')}, ${
        colors[0]
      })`,
      WebkitBackgroundClip: 'text',
      WebkitTextFillColor: 'transparent',
    };

    return (
      <span className={`relative inline-block ${className}`}>
        <span className="sr-only">{children}</span>
        <span
          aria-hidden="true"
          className="relative animate-aurora bg-[length:200%_auto] bg-clip-text text-transparent"
          style={gradientStyle}
        >
          {children}
        </span>
      </span>
    );
  },
);

AuroraText.displayName = 'AuroraText';
