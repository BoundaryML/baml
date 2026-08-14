'use client';

import { motion } from 'motion/react';
import React from 'react';
import { cn } from '@/lib/utils';

// Feature Item Component
interface FeatureItemProps {
  title: string;
  description: string;
  isActive?: boolean;
  onClick?: () => void;
}

export function FeatureItem({
  title,
  description,
  isActive = false,
  onClick,
}: FeatureItemProps) {
  return (
    <motion.div
      className={cn(
        'cursor-pointer transition-all duration-300',
        isActive ? 'mb-6' : 'mb-4',
      )}
      onClick={onClick}
      whileHover={{ scale: 1.02 }}
      whileTap={{ scale: 0.98 }}
    >
      {isActive ? (
        <div className="bg-muted/50 border border-border rounded-lg p-4 relative">
          <h3 className="text-lg font-semibold text-foreground mb-2">
            {title}
          </h3>
          <p className="text-sm text-muted-foreground leading-relaxed">
            {description}
          </p>
          <div className="absolute bottom-0 left-0 w-full h-0.5 bg-purple-600" />
        </div>
      ) : (
        <div className="p-2">
          <h3 className="text-sm text-muted-foreground font-medium">{title}</h3>
        </div>
      )}
    </motion.div>
  );
}

// 3D Architectural Rendering Component
interface ArchitecturalRenderingProps {
  className?: string;
}

export function ArchitecturalRendering({
  className,
}: ArchitecturalRenderingProps) {
  return (
    <div
      className={cn(
        'relative w-full h-full rounded-lg overflow-hidden border border-border bg-gradient-to-br from-white to-gray-100 dark:from-gray-900 dark:to-gray-800',
        className,
      )}
    >
      {/* 3D Architectural Scene */}
      <div className="absolute inset-0 flex items-center justify-center">
        <div className="relative w-80 h-64 bg-gradient-to-br from-blue-50 to-indigo-100 dark:from-blue-950 dark:to-indigo-900 rounded-lg shadow-2xl">
          {/* Upper Floor - Bedroom */}
          <div className="absolute top-0 left-0 right-0 h-1/2 bg-white dark:bg-gray-800 rounded-t-lg">
            {/* Bed */}
            <div className="absolute bottom-2 left-2 w-16 h-8 bg-gray-300 dark:bg-gray-600 rounded" />
            {/* Nightstand */}
            <div className="absolute bottom-2 left-20 w-6 h-6 bg-gray-400 dark:bg-gray-500 rounded" />
            {/* Window */}
            <div className="absolute top-2 right-2 w-12 h-8 bg-blue-200 dark:bg-blue-800 rounded border border-gray-300 dark:border-gray-600">
              <div className="w-full h-1/2 bg-gray-300 dark:bg-gray-600" />
            </div>
          </div>

          {/* Lower Floor - Living/Office */}
          <div className="absolute bottom-0 left-0 right-0 h-1/2 bg-gray-50 dark:bg-gray-700 rounded-b-lg">
            {/* Staircase */}
            <div className="absolute top-0 left-1/2 transform -translate-x-1/2 w-8 h-full">
              <div className="w-full h-1/2 bg-gray-400 dark:bg-gray-500 rounded-t-lg" />
              <div className="w-full h-1/2 bg-gray-300 dark:bg-gray-600 rounded-b-lg" />
            </div>

            {/* Desk */}
            <div className="absolute bottom-2 left-2 w-12 h-2 bg-brown-400 dark:bg-brown-600 rounded" />
            {/* Chair */}
            <div className="absolute bottom-4 left-4 w-4 h-4 bg-gray-400 dark:bg-gray-500 rounded-full" />
            {/* Lamp */}
            <div className="absolute bottom-6 left-8 w-1 h-6 bg-gray-400 dark:bg-gray-500">
              <div className="w-3 h-3 bg-yellow-300 dark:bg-yellow-600 rounded-full -top-1 -left-1 relative" />
            </div>
            {/* Bookshelf */}
            <div className="absolute bottom-2 right-2 w-8 h-8 bg-gray-400 dark:bg-gray-500 rounded">
              <div className="grid grid-cols-2 gap-1 p-1">
                <div className="w-full h-2 bg-gray-300 dark:bg-gray-600 rounded" />
                <div className="w-full h-2 bg-gray-300 dark:bg-gray-600 rounded" />
                <div className="w-full h-2 bg-gray-300 dark:bg-gray-600 rounded" />
                <div className="w-full h-2 bg-gray-300 dark:bg-gray-600 rounded" />
              </div>
            </div>
          </div>

          {/* Lighting Effects */}
          <div className="absolute inset-0 bg-gradient-to-t from-transparent via-transparent to-white/10 dark:to-black/10 pointer-events-none" />
        </div>
      </div>
    </div>
  );
}

// Feature List Component
interface FeatureListProps {
  features: Array<{
    id: string;
    title: string;
    description: string;
  }>;
  activeFeatureId?: string;
  onFeatureSelect?: (featureId: string) => void;
}

export function FeatureList({
  features,
  activeFeatureId,
  onFeatureSelect,
}: FeatureListProps) {
  return (
    <div className="flex flex-col">
      {features.map((feature) => (
        <FeatureItem
          description={feature.description}
          isActive={activeFeatureId === feature.id}
          key={feature.id}
          onClick={() => onFeatureSelect?.(feature.id)}
          title={feature.title}
        />
      ))}
    </div>
  );
}

// Main Feature Showcase Component
interface FeatureShowcaseProps {
  className?: string;
  features?: Array<{
    id: string;
    title: string;
    description: string;
  }>;
  defaultActiveFeatureId?: string;
  onFeatureSelect?: (featureId: string) => void;
  showArchitecturalRendering?: boolean;
}

export function FeatureShowcase({
  className,
  features = [
    {
      description:
        'Write BAML schemas with syntax highlighting, autocomplete, and validation in VS Code.',
      id: 'vscode',
      title: 'VS Code Extension',
    },
    {
      description:
        'Full BAML support for IntelliJ, WebStorm, and other JetBrains IDEs with advanced features.',
      id: 'jetbrains',
      title: 'JetBrains Plugin (coming soon)',
    },
    {
      description:
        'Generate TypeScript types and runtime validation from your BAML schemas automatically.',
      id: 'cli',
      title: 'BAML CLI',
    },
    {
      description:
        'Install BAML in your Node.js projects and generate types for your AI applications.',
      id: 'npm',
      title: 'NPM Package',
    },
  ],
  defaultActiveFeatureId = 'vscode',
  onFeatureSelect,
  showArchitecturalRendering = true,
}: FeatureShowcaseProps) {
  const [activeFeatureId, setActiveFeatureId] = React.useState(
    defaultActiveFeatureId,
  );

  const handleFeatureSelect = (featureId: string) => {
    setActiveFeatureId(featureId);
    onFeatureSelect?.(featureId);
  };

  return (
    <motion.div
      animate={{ opacity: 1, y: 0 }}
      className={cn('w-full max-w-6xl mx-auto', className)}
      initial={{ opacity: 0, y: 20 }}
      transition={{ duration: 0.5 }}
    >
      <div className="grid grid-cols-1 lg:grid-cols-5 gap-8 items-center">
        {/* Left Section - Feature List */}
        <div className="lg:col-span-2 space-y-4">
          <FeatureList
            activeFeatureId={activeFeatureId}
            features={features}
            onFeatureSelect={handleFeatureSelect}
          />
        </div>

        {/* Right Section - 3D Architectural Rendering */}
        {showArchitecturalRendering && (
          <div className="lg:col-span-3 h-80 lg:h-96">
            <ArchitecturalRendering />
          </div>
        )}
      </div>
    </motion.div>
  );
}

// Compact Feature Showcase for limited vertical space
interface CompactFeatureShowcaseProps {
  className?: string;
  features?: Array<{
    id: string;
    title: string;
    description: string;
  }>;
  defaultActiveFeatureId?: string;
  onFeatureSelect?: (featureId: string) => void;
}

export function CompactFeatureShowcase({
  className,
  features = [
    {
      description:
        'Write BAML schemas with syntax highlighting, autocomplete, and validation in VS Code.',
      id: 'vscode',
      title: 'VS Code Extension',
    },
    {
      description:
        'Full BAML support for IntelliJ, WebStorm, and other JetBrains IDEs with advanced features.',
      id: 'jetbrains',
      title: 'JetBrains Plugin (coming soon)',
    },
    {
      description:
        'Generate TypeScript types and runtime validation from your BAML schemas automatically.',
      id: 'cli',
      title: 'BAML CLI',
    },
    {
      description:
        'Install BAML in your Node.js projects and generate types for your AI applications.',
      id: 'npm',
      title: 'NPM Package',
    },
  ],
  defaultActiveFeatureId = 'vscode',
  onFeatureSelect,
}: CompactFeatureShowcaseProps) {
  const [activeFeatureId, setActiveFeatureId] = React.useState(
    defaultActiveFeatureId,
  );

  const handleFeatureSelect = (featureId: string) => {
    setActiveFeatureId(featureId);
    onFeatureSelect?.(featureId);
  };

  const activeFeature = features.find((f) => f.id === activeFeatureId);

  return (
    <motion.div
      animate={{ opacity: 1, y: 0 }}
      className={cn('w-full max-w-4xl mx-auto', className)}
      initial={{ opacity: 0, y: 20 }}
      transition={{ duration: 0.5 }}
    >
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 items-center">
        {/* Left Section - Feature List */}
        <div className="lg:col-span-1 space-y-2">
          {features.map((feature) => (
            <FeatureItem
              description={feature.description}
              isActive={activeFeatureId === feature.id}
              key={feature.id}
              onClick={() => handleFeatureSelect(feature.id)}
              title={feature.title}
            />
          ))}
        </div>

        {/* Right Section - Active Feature Content */}
        <div className="lg:col-span-2 h-64">
          <ArchitecturalRendering />
        </div>
      </div>
    </motion.div>
  );
}
