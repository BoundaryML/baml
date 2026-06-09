'use client';

import {
  Code,
  Copy,
  Download,
  Play,
  Settings,
  Share2,
  Sparkles,
  Terminal,
} from 'lucide-react';
import Link from 'next/link';
import { useState } from 'react';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';

const exampleCode = `// Define your AI interface with BAML
class User {
  name: string
  email: string
  age: int
}

function GetUserInfo(query: string) -> User {
  client GPT4
  prompt #"
    Extract user information from: {{query}}

    Return as a User object.
  "#
}

// Use it with full type safety
const user = await GetUserInfo("John Doe, john@example.com, 28 years old")
console.log(user.name) // TypeScript knows this is a string!`;

const examples = [
  {
    category: 'Data Extraction',
    description: 'Parse unstructured text into typed objects',
    id: 1,
    title: 'Extract Structured Data',
  },
  {
    category: 'Workflows',
    description: 'Chain multiple AI calls with type safety',
    id: 2,
    title: 'Multi-step AI Workflow',
  },
  {
    category: 'Generation',
    description: 'Generate content with validated schemas',
    id: 3,
    title: 'Content Generation',
  },
  {
    category: 'Classification',
    description: 'Build reliable classification systems',
    id: 4,
    title: 'Classification Pipeline',
  },
];

export default function PlaygroundPage() {
  const [activeTab, setActiveTab] = useState('code');
  const [isRunning, setIsRunning] = useState(false);

  const handleRun = () => {
    setIsRunning(true);
    setTimeout(() => {
      setIsRunning(false);
      setActiveTab('output');
    }, 2000);
  };

  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <Navbar />
      <main className="flex flex-col items-center justify-center divide-y divide-border min-h-screen w-full">
        {/* Hero Section */}
        <section className="relative flex w-full items-center justify-center px-4 py-12 md:py-20">
          <div className="flex flex-col items-center justify-center space-y-6 text-center">
            <div className="flex items-center gap-2 rounded-full bg-primary/10 px-4 py-2 text-sm font-medium">
              <Sparkles className="h-4 w-4" />
              Interactive Playground
            </div>
            <h1 className="text-3xl font-bold tracking-tight sm:text-4xl md:text-5xl lg:text-6xl max-w-4xl">
              Try BAML in Your Browser
            </h1>
            <p className="max-w-2xl text-lg text-muted-foreground">
              Experiment with BAML code, test AI prompts, and see type-safe
              results instantly. No installation required.
            </p>
          </div>
        </section>

        {/* Playground Section */}
        <section className="w-full px-4 py-8">
          <div className="mx-auto max-w-7xl">
            <div className="grid lg:grid-cols-[300px_1fr] gap-6">
              {/* Sidebar */}
              <div className="space-y-6">
                <Card className="p-4">
                  <h3 className="font-semibold mb-3">Examples</h3>
                  <div className="space-y-2">
                    {examples.map((example) => (
                      <button
                        className="w-full text-left p-3 rounded-md hover:bg-muted transition-colors"
                        key={example.id}
                      >
                        <p className="font-medium text-sm">{example.title}</p>
                        <p className="text-xs text-muted-foreground">
                          {example.description}
                        </p>
                        <span className="text-xs bg-primary/10 px-2 py-0.5 rounded-full mt-1 inline-block">
                          {example.category}
                        </span>
                      </button>
                    ))}
                  </div>
                </Card>

                <Card className="p-4">
                  <h3 className="font-semibold mb-3">Quick Actions</h3>
                  <div className="space-y-2">
                    <Button
                      className="w-full justify-start gap-2"
                      size="sm"
                      variant="outline"
                    >
                      <Copy className="h-4 w-4" />
                      Copy Code
                    </Button>
                    <Button
                      className="w-full justify-start gap-2"
                      size="sm"
                      variant="outline"
                    >
                      <Download className="h-4 w-4" />
                      Download Example
                    </Button>
                    <Button
                      className="w-full justify-start gap-2"
                      size="sm"
                      variant="outline"
                    >
                      <Share2 className="h-4 w-4" />
                      Share Playground
                    </Button>
                  </div>
                </Card>
              </div>

              {/* Main Editor Area */}
              <div className="space-y-4">
                <Card className="overflow-hidden">
                  <div className="border-b border-border bg-muted/50 px-4 py-2">
                    <div className="flex items-center justify-between">
                      <div className="flex gap-2">
                        <button
                          className={`px-3 py-1 text-sm font-medium rounded-md transition-colors ${
                            activeTab === 'code'
                              ? 'bg-background border border-border'
                              : 'text-muted-foreground hover:text-foreground'
                          }`}
                          onClick={() => setActiveTab('code')}
                        >
                          <Code className="h-4 w-4 inline mr-1" />
                          BAML Code
                        </button>
                        <button
                          className={`px-3 py-1 text-sm font-medium rounded-md transition-colors ${
                            activeTab === 'output'
                              ? 'bg-background border border-border'
                              : 'text-muted-foreground hover:text-foreground'
                          }`}
                          onClick={() => setActiveTab('output')}
                        >
                          <Terminal className="h-4 w-4 inline mr-1" />
                          Output
                        </button>
                      </div>
                      <Button size="sm" variant="ghost">
                        <Settings className="h-4 w-4" />
                      </Button>
                    </div>
                  </div>

                  <div className="p-4 min-h-[500px]">
                    {activeTab === 'code' ? (
                      <div className="font-mono text-sm">
                        <pre className="whitespace-pre-wrap">{exampleCode}</pre>
                      </div>
                    ) : (
                      <div className="font-mono text-sm">
                        <div className="text-green-600 dark:text-green-400">
                          ✓ BAML code validated successfully
                        </div>
                        <div className="mt-4">
                          <div className="text-muted-foreground">
                            // Output:
                          </div>
                          <div className="mt-2">
                            {`{
  "name": "John Doe",
  "email": "john@example.com",
  "age": 28
}`}
                          </div>
                        </div>
                        <div className="mt-4 text-muted-foreground text-xs">
                          Execution time: 1.23s | Model: GPT-4 | Tokens: 127
                        </div>
                      </div>
                    )}
                  </div>
                </Card>

                <div className="flex items-center justify-between">
                  <div className="flex gap-2">
                    <Button
                      className="gap-2"
                      disabled={isRunning}
                      onClick={handleRun}
                    >
                      <Play className="h-4 w-4" />
                      {isRunning ? 'Running...' : 'Run Code'}
                    </Button>
                    <Button variant="outline">Clear</Button>
                  </div>
                  <div className="text-sm text-muted-foreground">
                    Playground powered by BAML runtime v2.0
                  </div>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* Features Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="text-center mb-12">
              <h2 className="text-3xl font-bold mb-4">Playground Features</h2>
              <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
                Everything you need to experiment with BAML and build confidence
                before implementing
              </p>
            </div>
            <div className="grid md:grid-cols-3 gap-6">
              <Card className="p-6">
                <div className="h-12 w-12 rounded-lg bg-primary/10 flex items-center justify-center mb-4">
                  <Code className="h-6 w-6 text-primary" />
                </div>
                <h3 className="text-xl font-semibold mb-2">Live Validation</h3>
                <p className="text-muted-foreground">
                  Get instant feedback on your BAML code with real-time syntax
                  checking and validation.
                </p>
              </Card>
              <Card className="p-6">
                <div className="h-12 w-12 rounded-lg bg-primary/10 flex items-center justify-center mb-4">
                  <Sparkles className="h-6 w-6 text-primary" />
                </div>
                <h3 className="text-xl font-semibold mb-2">Example Library</h3>
                <p className="text-muted-foreground">
                  Start with pre-built examples for common use cases and modify
                  them to fit your needs.
                </p>
              </Card>
              <Card className="p-6">
                <div className="h-12 w-12 rounded-lg bg-primary/10 flex items-center justify-center mb-4">
                  <Share2 className="h-6 w-6 text-primary" />
                </div>
                <h3 className="text-xl font-semibold mb-2">Share & Export</h3>
                <p className="text-muted-foreground">
                  Share your playground sessions with teammates or export code
                  to use in your projects.
                </p>
              </Card>
            </div>
          </div>
        </section>

        {/* CTA Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-4xl text-center">
            <h2 className="text-3xl font-bold mb-4">Ready to Build?</h2>
            <p className="text-lg text-muted-foreground mb-8 max-w-2xl mx-auto">
              Install BAML in your project and start building type-safe AI
              applications today.
            </p>
            <div className="flex flex-col sm:flex-row gap-4 justify-center">
              <Button asChild size="lg">
                <Link href="/docs/getting-started">Get Started</Link>
              </Button>
              <Button asChild size="lg" variant="outline">
                <Link href="https://marketplace.visualstudio.com/items?itemName=boundaryml.baml">
                  Install VS Code Extension
                </Link>
              </Button>
            </div>
          </div>
        </section>

        <FooterSection />
      </main>
    </div>
  );
}
