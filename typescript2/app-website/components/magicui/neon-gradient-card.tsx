'use client';

import type { CSSProperties, ReactElement, ReactNode } from 'react';
import { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

interface NeonColorsProps {
  firstColor: string;
  secondColor: string;
}

interface NeonGradientCardProps {
  /**
   * @default <div />
   * @type ReactElement
   * @description
   * The component to be rendered as the card
   * */
  as?: ReactElement;
  /**
   * @default ""
   * @type string
   * @description
   * The className of the card
   */
  className?: string;

  /**
   * @default ""
   * @type ReactNode
   * @description
   * The children of the card
   * */
  children?: ReactNode;

  /**
   * @default 5
   * @type number
   * @description
   * The size of the border in pixels
   * */
  borderSize?: number;

  /**
   * @default 20
   * @type number
   * @description
   * The size of the radius in pixels
   * */
  borderRadius?: number;

  /**
   * @default "{ firstColor: '#ff00aa', secondColor: '#00FFF1' }"
   * @type string
   * @description
   * The colors of the neon gradient
   * */
  neonColors?: NeonColorsProps;

  [key: string]: unknown;
}

const NeonGradientCard: React.FC<NeonGradientCardProps> = ({
  className,
  children,
  borderSize = 2,
  borderRadius = 20,
  neonColors = {
    firstColor: '#84a6d3',
    secondColor: '#75a99c',
  },
  ...props
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const [dimensions, setDimensions] = useState({ height: 0, width: 0 });

  useEffect(() => {
    const updateDimensions = () => {
      if (containerRef.current) {
        const { offsetWidth, offsetHeight } = containerRef.current;
        setDimensions({ height: offsetHeight, width: offsetWidth });
      }
    };

    updateDimensions();
    window.addEventListener('resize', updateDimensions);

    return () => {
      window.removeEventListener('resize', updateDimensions);
    };
  }, []);

  useEffect(() => {
    if (containerRef.current) {
      const { offsetWidth, offsetHeight } = containerRef.current;
      setDimensions({ height: offsetHeight, width: offsetWidth });
    }
  }, []);

  return (
    <div
      className={cn(
        'relative z-10 h-full w-full rounded-[var(--border-radius)]',
        className,
      )}
      ref={containerRef}
      style={
        {
          '--after-blur': `${dimensions.width / 3}px`,
          '--border-radius': `${borderRadius}px`,
          '--border-size': `${borderSize}px`,
          '--card-content-radius': `${borderRadius - borderSize}px`,
          '--card-height': `${dimensions.height}px`,
          '--card-width': `${dimensions.width}px`,
          '--neon-first-color': neonColors.firstColor,
          '--neon-second-color': neonColors.secondColor,
          '--pseudo-element-background-image': `linear-gradient(0deg, ${neonColors.firstColor}, ${neonColors.secondColor})`,
          '--pseudo-element-height': `${dimensions.height + borderSize * 2}px`,
          '--pseudo-element-width': `${dimensions.width + borderSize * 2}px`,
        } as CSSProperties
      }
      {...props}
    >
      <div
        className={cn(
          'relative h-full min-h-[inherit] w-full rounded-[var(--card-content-radius)] bg-gray-100 p-6',
          'before:absolute before:-left-[var(--border-size)] before:-top-[var(--border-size)] before:-z-10 before:block',
          "before:h-[var(--pseudo-element-height)] before:w-[var(--pseudo-element-width)] before:rounded-[var(--border-radius)] before:content-['']",
          'before:bg-[linear-gradient(0deg,var(--neon-first-color),var(--neon-second-color))] before:bg-[length:100%_200%]',
          'before:animate-backgroundPositionSpin',
          'after:absolute after:-left-[var(--border-size)] after:-top-[var(--border-size)] after:-z-10 after:block',
          "after:h-[var(--pseudo-element-height)] after:w-[var(--pseudo-element-width)] after:rounded-[var(--border-radius)] after:blur-[var(--after-blur)] after:content-['']",
          'after:bg-[linear-gradient(0deg,var(--neon-first-color),var(--neon-second-color))] after:bg-[length:100%_200%] after:opacity-80',
          'after:animate-backgroundPositionSpin',
          'dark:bg-neutral-900',
        )}
      >
        {children}
      </div>
    </div>
  );
};

export { NeonGradientCard };
