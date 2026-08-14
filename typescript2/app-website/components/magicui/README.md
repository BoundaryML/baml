# Showcase Components

This directory contains modular showcase components that can be used together or separately in different parts of the marketing site.

## Components Overview

### 1. Code Analysis Interface (`code-analysis-interface.tsx`)

A modular component that recreates the code analysis tool interface from the first screenshot, broken down into sub-components:

- **`CodeAnalysisHeader`**: Top navigation bar with project selector, model selector, and action buttons
- **`SystemPrompt`**: System prompt section with tabs, code example, and JSON schema
- **`History`**: History section with success/failure counters and playback controls
- **`Results`**: Results section showing parsed response with status indicators
- **`CodeAnalysisInterface`**: Main component that combines all sub-components

### 2. Feature Showcase (`feature-showcase.tsx`)

A component that recreates the feature showcase from the second screenshot:

- **`FeatureItem`**: Individual feature item with active/inactive states
- **`ArchitecturalRendering`**: 3D architectural rendering component
- **`FeatureList`**: List of feature items
- **`FeatureShowcase`**: Full feature showcase with architectural rendering
- **`CompactFeatureShowcase`**: Compact version for limited vertical space

### 3. Combined Showcase (`combined-showcase.tsx`)

A wrapper component that combines both showcases with flexible configuration:

- **`CombinedShowcase`**: Main component with variant options
- **`CodeAnalysisShowcase`**: Individual code analysis component
- **`FeatureShowcaseOnly`**: Individual feature showcase component
- **`ShowcaseSection`**: Section wrapper with title and description
- **`CodeAnalysisSection`**: Pre-configured code analysis section
- **`FeatureShowcaseSection`**: Pre-configured feature showcase section
- **`CombinedShowcaseSection`**: Pre-configured combined section

## Usage Examples

### Basic Usage

```tsx
import { CombinedShowcaseSection } from '@/components/magicui/combined-showcase';

// Full combined showcase
<CombinedShowcaseSection
  title="Complete Development Experience"
  description="From code analysis to feature development, we've got you covered"
  variant="full"
/>

// Compact combined showcase for limited vertical space
<CombinedShowcaseSection
  title="Development Tools"
  description="Choose the right tool for your workflow"
  variant="compact"
/>
```

### Individual Components

```tsx
import { CodeAnalysisSection, FeatureShowcaseSection } from '@/components/magicui/combined-showcase';

// Code analysis only
<CodeAnalysisSection
  title="AI-Powered Code Analysis"
  description="Experience our powerful code analysis tool with type-safe AI development"
/>

// Feature showcase only (compact)
<FeatureShowcaseSection
  title="Development Tools"
  description="Choose the right tool for your development workflow"
  compact={true}
/>
```

### Custom Configuration

```tsx
import { CombinedShowcase } from '@/components/magicui/combined-showcase';

<CombinedShowcase
  variant="full"
  codeAnalysisProps={{
    headerProps: {
      projectName: "CustomProject",
      modelName: "GPT-4"
    },
    systemPromptProps: {
      systemPrompt: "Analyze this custom codebase:",
      codeExample: `function customFunction(input: string): string {
  return input.toUpperCase();
}`
    }
  }}
  featureShowcaseProps={{
    features: [
      {
        id: 'custom1',
        title: 'Custom Tool 1',
        description: 'This is a custom development tool with specific features.'
      },
      {
        id: 'custom2',
        title: 'Custom Tool 2',
        description: 'Another custom tool for different development needs.'
      }
    ],
    defaultActiveFeatureId: 'custom1'
  }}
/>
```

### Integration with Existing Feature Section

```tsx
import { FeatureShowcaseOnly } from '@/components/magicui/combined-showcase';

<section className="py-16 lg:py-24" id="features">
  <div className="container mx-auto px-4">
    <div className="text-center mb-12">
      <h2 className="text-3xl md:text-4xl font-bold tracking-tight mb-4">
        Simple. Type-Safe. Reliable.
      </h2>
      <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
        Discover how BAML transforms AI development in four easy steps
      </p>
    </div>

    <div className="w-full h-full lg:h-[450px] flex items-center justify-center">
      <FeatureShowcaseOnly
        compact={false}
        features={[
          {
            id: 'vscode',
            title: 'VS Code Extension',
            description: 'Write BAML schemas with syntax highlighting, autocomplete, and validation in VS Code.'
          },
          {
            id: 'jetbrains',
            title: 'JetBrains Plugin (coming soon)',
            description: 'Full BAML support for IntelliJ, WebStorm, and other JetBrains IDEs with advanced features.'
          },
          {
            id: 'cli',
            title: 'BAML CLI',
            description: 'Generate TypeScript types and runtime validation from your BAML schemas automatically.'
          },
          {
            id: 'npm',
            title: 'NPM Package',
            description: 'Install BAML in your Node.js projects and generate types for your AI applications.'
          }
        ]}
        defaultActiveFeatureId="vscode"
      />
    </div>
  </div>
</section>
```

## Component Variants

### Combined Showcase Variants

- **`full`**: Shows both code analysis and feature showcase in full layout
- **`compact`**: Shows both components in compact layout for limited vertical space
- **`code-only`**: Shows only the code analysis interface
- **`feature-only`**: Shows only the feature showcase

### Feature Showcase Variants

- **`FeatureShowcase`**: Full version with architectural rendering
- **`CompactFeatureShowcase`**: Compact version for limited vertical space

## Props Reference

### CodeAnalysisInterface Props

```tsx
interface CodeAnalysisInterfaceProps {
  className?: string;
  headerProps?: Partial<CodeAnalysisHeaderProps>;
  systemPromptProps?: Partial<SystemPromptProps>;
  historyProps?: Partial<HistoryProps>;
  resultsProps?: Partial<ResultsProps>;
}
```

### FeatureShowcase Props

```tsx
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
```

### CombinedShowcase Props

```tsx
interface CombinedShowcaseProps {
  className?: string;
  variant?: 'full' | 'compact' | 'code-only' | 'feature-only';
  showCodeAnalysis?: boolean;
  showFeatureShowcase?: boolean;
  codeAnalysisProps?: Record<string, unknown>;
  featureShowcaseProps?: Record<string, unknown>;
}
```

## Styling

All components use Tailwind CSS classes and are designed to work with the existing design system. They support both light and dark themes and are fully responsive.

## Examples

See `components/examples/showcase-examples.tsx` for comprehensive usage examples.