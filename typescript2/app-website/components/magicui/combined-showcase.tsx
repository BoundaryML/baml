'use client';

import { motion } from 'motion/react';
import type React from 'react';
import { cn } from '@/lib/utils';
import { CodeAnalysisInterface } from './code-analysis-interface';
import { CompactFeatureShowcase, FeatureShowcase } from './feature-showcase';

// Combined Showcase Component
interface CombinedShowcaseProps {
  className?: string;
  variant?: 'full' | 'compact' | 'code-only' | 'feature-only';
  showCodeAnalysis?: boolean;
  showFeatureShowcase?: boolean;
  codeAnalysisProps?: Record<string, unknown>;
  featureShowcaseProps?: Record<string, unknown>;
}

export function CombinedShowcase({
  className,
  variant = 'full',
  showCodeAnalysis = true,
  showFeatureShowcase = true,
  codeAnalysisProps,
  featureShowcaseProps,
}: CombinedShowcaseProps) {
  const renderCodeAnalysis = () => {
    if (!showCodeAnalysis) return null;

    return (
      <motion.div
        animate={{ opacity: 1, y: 0 }}
        className="w-full"
        initial={{ opacity: 0, y: 20 }}
        transition={{ delay: 0.1, duration: 0.5 }}
      >
        <CodeAnalysisInterface {...codeAnalysisProps} />
      </motion.div>
    );
  };

  const renderFeatureShowcase = () => {
    if (!showFeatureShowcase) return null;

    const FeatureComponent =
      variant === 'compact' ? CompactFeatureShowcase : FeatureShowcase;

    return (
      <motion.div
        animate={{ opacity: 1, y: 0 }}
        className="w-full"
        initial={{ opacity: 0, y: 20 }}
        transition={{ delay: 0.2, duration: 0.5 }}
      >
        <FeatureComponent {...featureShowcaseProps} />
      </motion.div>
    );
  };

  if (variant === 'code-only') {
    return (
      <div className={cn('w-full', className)}>{renderCodeAnalysis()}</div>
    );
  }

  if (variant === 'feature-only') {
    return (
      <div className={cn('w-full', className)}>{renderFeatureShowcase()}</div>
    );
  }

  return (
    <div className={cn('w-full space-y-12', className)}>
      {renderCodeAnalysis()}
      {renderFeatureShowcase()}
    </div>
  );
}

// Individual Components for separate use
export function CodeAnalysisShowcase({
  className,
  ...props
}: {
  className?: string;
  [key: string]: unknown;
}) {
  return (
    <div className={cn('w-full', className)}>
      <CodeAnalysisInterface {...props} />
    </div>
  );
}

export function FeatureShowcaseOnly({
  className,
  compact = false,
  ...props
}: {
  className?: string;
  compact?: boolean;
  [key: string]: unknown;
}) {
  const FeatureComponent = compact ? CompactFeatureShowcase : FeatureShowcase;

  return (
    <div className={cn('w-full', className)}>
      <FeatureComponent {...props} />
    </div>
  );
}

// Section Wrapper Components
interface ShowcaseSectionProps {
  className?: string;
  title?: string;
  description?: string;
  children: React.ReactNode;
}

export function ShowcaseSection({
  className,
  title,
  description,
  children,
}: ShowcaseSectionProps) {
  return (
    <section className={cn('py-16 lg:py-24', className)}>
      <div className="container mx-auto px-4">
        {(title || description) && (
          <div className="text-center mb-12">
            {title && (
              <h2 className="text-3xl md:text-4xl font-bold tracking-tight mb-4">
                {title}
              </h2>
            )}
            {description && (
              <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
                {description}
              </p>
            )}
          </div>
        )}
        {children}
      </div>
    </section>
  );
}

// Pre-configured sections
export function CodeAnalysisSection({
  className,
  title = 'Code Analysis Interface',
  description = 'Experience our powerful code analysis tool with type-safe AI development',
  ...props
}: {
  className?: string;
  title?: string;
  description?: string;
  [key: string]: unknown;
}) {
  return (
    <ShowcaseSection
      className={className}
      description={description}
      title={title}
    >
      <CodeAnalysisShowcase {...props} />
    </ShowcaseSection>
  );
}

export function FeatureShowcaseSection({
  className,
  title = 'Development Tools',
  description = 'Choose the right tool for your development workflow',
  compact = false,
  ...props
}: {
  className?: string;
  title?: string;
  description?: string;
  compact?: boolean;
  [key: string]: any;
}) {
  return (
    <ShowcaseSection
      className={className}
      description={description}
      title={title}
    >
      <FeatureShowcaseOnly compact={compact} {...props} />
    </ShowcaseSection>
  );
}

export function CombinedShowcaseSection({
  className,
  title = 'Complete Development Experience',
  description = "From code analysis to feature development, we've got you covered",
  variant = 'full',
  ...props
}: {
  className?: string;
  title?: string;
  description?: string;
  variant?: 'full' | 'compact';
  [key: string]: any;
}) {
  return (
    <ShowcaseSection
      className={className}
      description={description}
      title={title}
    >
      <CombinedShowcase variant={variant} {...props} />
    </ShowcaseSection>
  );
}
