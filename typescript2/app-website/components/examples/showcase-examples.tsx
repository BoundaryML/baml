'use client';

import {
  CodeAnalysisSection,
  CodeAnalysisShowcase,
  CombinedShowcase,
  CombinedShowcaseSection,
  FeatureShowcaseOnly,
  FeatureShowcaseSection,
} from '../magicui/combined-showcase';

// Example 1: Full combined showcase
export function FullShowcaseExample() {
  return (
    <CombinedShowcaseSection
      description="From code analysis to feature development, we've got you covered"
      title="Complete Development Experience"
      variant="full"
    />
  );
}

// Example 2: Compact combined showcase for limited vertical space
export function CompactShowcaseExample() {
  return (
    <CombinedShowcaseSection
      description="Choose the right tool for your workflow"
      title="Development Tools"
      variant="compact"
    />
  );
}

// Example 3: Code analysis only
export function CodeAnalysisOnlyExample() {
  return (
    <CodeAnalysisSection
      description="Experience our powerful code analysis tool with type-safe AI development"
      title="AI-Powered Code Analysis"
    />
  );
}

// Example 4: Feature showcase only (compact)
export function FeatureShowcaseOnlyExample() {
  return (
    <FeatureShowcaseSection
      compact={true}
      description="Choose the right tool for your development workflow"
      title="Development Tools"
    />
  );
}

// Example 5: Custom combined showcase with specific props
export function CustomShowcaseExample() {
  return (
    <CombinedShowcase
      codeAnalysisProps={{
        headerProps: {
          modelName: 'GPT-4',
          projectName: 'CustomProject',
        },
        systemPromptProps: {
          codeExample: `function customFunction(input: string): string {
  return input.toUpperCase();
}`,
          systemPrompt: 'Analyze this custom codebase:',
        },
      }}
      featureShowcaseProps={{
        defaultActiveFeatureId: 'custom1',
        features: [
          {
            description:
              'This is a custom development tool with specific features.',
            id: 'custom1',
            title: 'Custom Tool 1',
          },
          {
            description: 'Another custom tool for different development needs.',
            id: 'custom2',
            title: 'Custom Tool 2',
          },
        ],
      }}
      variant="full"
    />
  );
}

// Example 6: Individual components usage
export function IndividualComponentsExample() {
  return (
    <div className="space-y-16">
      {/* Code Analysis Component */}
      <div>
        <h2 className="text-2xl font-bold mb-4">Code Analysis Interface</h2>
        <CodeAnalysisShowcase
          headerProps={{
            modelName: 'Claude-3',
            projectName: 'ExampleProject',
          }}
        />
      </div>

      {/* Feature Showcase Component */}
      <div>
        <h2 className="text-2xl font-bold mb-4">Development Tools</h2>
        <FeatureShowcaseOnly
          compact={false}
          features={[
            {
              description:
                'Enhanced development experience with syntax highlighting and autocomplete.',
              id: 'tool1',
              title: 'VS Code Extension',
            },
            {
              description:
                'Command-line interface for quick development tasks.',
              id: 'tool2',
              title: 'CLI Tool',
            },
          ]}
        />
      </div>
    </div>
  );
}

// Example 7: Compact feature showcase for marketing sections
export function MarketingFeatureExample() {
  return (
    <div className="py-12">
      <div className="text-center mb-8">
        <h2 className="text-3xl font-bold mb-4">Development Tools</h2>
        <p className="text-lg text-muted-foreground">
          Choose the right tool for your development workflow
        </p>
      </div>

      <FeatureShowcaseOnly
        compact={true}
        defaultActiveFeatureId="vscode"
        features={[
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
        ]}
      />
    </div>
  );
}

// Example 8: Integration with existing feature section
export function IntegrationExample() {
  return (
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
            defaultActiveFeatureId="vscode"
            features={[
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
            ]}
          />
        </div>
      </div>
    </section>
  );
}
