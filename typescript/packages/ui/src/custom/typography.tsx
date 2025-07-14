import type { VariantProps } from 'class-variance-authority';
import { cva } from 'class-variance-authority';
import type { TwcComponentProps } from 'react-twc';

import { cn, twx } from '@baml/ui/lib/utils';

import { Skeleton } from '../components/skeleton';

export const typographyVariants = cva(undefined, {
  variants: {
    variant: {
      destructive: 'text-destructive',
      muted: 'text-muted-foreground',
      primary: 'text-primary',
      'primary-darker': 'text-primary-darker',
      'primary-foreground': 'text-primary-foreground',
      'script-elegant': 'font-serif text-secondary-foreground',
      'script-mono': 'font-script text-secondary-foreground',
      'script-playful': 'font-sans text-secondary-foreground',
      'script-simple': 'font-sans text-secondary-foreground',
      secondary: 'text-secondary-foreground',
      warning: 'text-warning',
    },
  },
});

export const proseTypographyVariants = cva('prose', {
  defaultVariants: {
    size: 'default',
    textFlow: 'pretty',
  },
  variants: {
    size: {
      default: 'prose-base',
      lg: 'prose-lg',
      sm: 'prose-sm',
      xl: 'prose-xl',
      xs: 'prose-xs',
    },
    textFlow: {
      noWrap: 'whitespace-nowrap',
      pretty: 'text-pretty',
      truncate: 'truncate',
    },
    variant: {
      'primary-foreground': 'prose-primary-foreground',
    },
  },
});

export const textVariants = cva(undefined, {
  defaultVariants: {
    size: 'default',
    spacing: 'default',
    textFlow: 'pretty',
  },
  variants: {
    size: {
      default: 'text-base',
      lg: 'font-semibold text-lg',
      script: 'text-[15px]',
      sm: 'text-sm',
      xl: 'text-xl',
      xs: 'text-xs',
    },
    spacing: {
      '2xl': 'leading-9',
      '3xl': 'leading-10',
      default: 'leading-5',
      lg: 'leading-7',
      script: 'leading-4',
      sm: 'leading-4',
      xl: 'leading-8',
      xs: 'leading-3',
    },
    textFlow: {
      noWrap: 'whitespace-nowrap',
      pretty: 'text-pretty',
      truncate: 'truncate',
    },
  },
});

export const textLoadingVariants = cva('', {
  defaultVariants: {
    size: 'default',
  },
  variants: {
    size: {
      default: 'text-base',
      lg: 'text-lg',
      sm: 'text-sm',
      xl: 'text-xl',
      xs: 'text-xs',
    },
  },
});

export type TypographyProps = VariantProps<typeof typographyVariants>;
export type TextTypographyProps = TypographyProps &
  VariantProps<typeof textVariants>;

export const H1 = twx.h1.transientProps(['variant'])<
  TypographyProps & TwcComponentProps<'h1'>
>(({ variant }) =>
  cn(
    typographyVariants({ variant }),
    'text-balance font-semibold text-4xl tracking-tight',
  ),
);

export const H2 = twx.h2.transientProps(['variant'])<
  TypographyProps & TwcComponentProps<'h2'>
>(({ variant }) =>
  cn(
    typographyVariants({ variant }),
    'text-balance font-semibold text-3xl tracking-tight first:mt-0',
  ),
);

export const H3 = twx.h3.transientProps(['variant'])<
  TypographyProps & TwcComponentProps<'h3'>
>(({ variant }) =>
  cn(
    typographyVariants({ variant }),
    'text-balance font-semibold text-2xl tracking-tight',
  ),
);
export const H4 = twx.h4.transientProps(['variant'])<
  TypographyProps & TwcComponentProps<'h4'>
>(({ variant }) =>
  cn(
    typographyVariants({ variant }),
    'text-balance font-semibold text-2xl leading-8',
  ),
);
export const P = twx.p.transientProps([
  'variant',
  'size',
  'textFlow',
  'spacing',
])<TextTypographyProps & TwcComponentProps<'p'>>(
  ({ size, variant, textFlow }) =>
    cn(typographyVariants({ variant }), textVariants({ size, textFlow })),
);

export const Blockquote = twx.blockquote`mt-6 space-y-2 text-pretty border-l-2 pl-6`;
export const BlockquoteContent = twx.p`text-pretty text-lg italic`;
export const BlockquoteFooter = twx.footer`text-pretty text-sm`;
export const List = twx.ul`my-6 ml-6 list-disc [&>li]:mt-2`;
export const ListItem = twx.li`text-pretty leading-6`;
export const Code = twx.code`relative rounded bg-muted px-[0.3rem] py-[0.2rem] font-mono text-sm font-semibold`;
export const Lead = twx.p`text-pretty text-xl text-muted-foreground`;
export const Kbd = twx.kbd`bg-dark text-dark-foreground pointer-events-none hidden size-5 select-none items-center justify-center rounded border font-mono text-xs font-semibold tracking-widest shadow-2xs lg:inline-flex`;
export const Abbr = twx.abbr`no-underline`;

export const Prose = twx.span.transientProps(['size', 'variant', 'textFlow'])<
  VariantProps<typeof proseTypographyVariants> & TwcComponentProps<'div'>
>(({ size, variant }) =>
  cn('prose', proseTypographyVariants({ variant }), textVariants({ size })),
);

export const Text = twx.span.transientProps([
  'size',
  'variant',
  'textFlow',
  'spacing',
])<TextTypographyProps & TwcComponentProps<'div'>>(
  ({ size, variant, textFlow, spacing }) =>
    cn(
      typographyVariants({ variant }),
      textVariants({ size, spacing, textFlow }),
    ),
);

export type TextLoadingProps = TwcComponentProps<'div'> &
  VariantProps<typeof textLoadingVariants>;

export const TextLoading = twx(Skeleton).transientProps([
  'size',
])<TextLoadingProps>(({ size }) => textLoadingVariants({ size }));
