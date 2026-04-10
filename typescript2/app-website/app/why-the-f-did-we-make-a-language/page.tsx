import {
  AlertTriangle,
  Brain,
  Code,
  Play,
  Rocket,
  Shield,
  Zap,
} from 'lucide-react';
import Link from 'next/link';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';

const problems = [
  {
    icon: AlertTriangle,
    problem: 'JSON schemas and untyped API responses everywhere',
    solution: 'First-class types for AI inputs and outputs',
    title: 'Type Safety Nightmare',
  },
  {
    icon: Code,
    problem: 'String concatenation and template literals for prompts',
    solution: 'Structured prompt composition with validation',
    title: 'Prompt Engineering Hell',
  },
  {
    icon: Brain,
    problem: 'Writing prompts in strings with no autocomplete or validation',
    solution: 'Full IDE support with syntax highlighting and IntelliSense',
    title: 'No IDE Support',
  },
  {
    icon: Shield,
    problem: "AI responses that don't match expected formats",
    solution: 'Compile-time validation and runtime guarantees',
    title: 'Runtime Failures',
  },
];

const comparisons = [
  {
    code: `// Hope the AI returns the right format 🤞
const response = await openai.complete({
  prompt: \`Extract user info from: \${text}\`
});

// Runtime error waiting to happen
const user = JSON.parse(response);
console.log(user.name); // undefined? string? who knows!`,
    problems: [
      'No type safety',
      'Runtime errors',
      'Manual parsing',
      'No validation',
    ],
    title: 'Without BAML',
  },
  {
    benefits: [
      'Full type safety',
      'Compile-time checks',
      'Auto parsing',
      'Built-in validation',
    ],
    code: `// Define once, use everywhere with confidence
class User {
  name: string
  email: string
  age: int
}

const user = await GetUserInfo(text);
console.log(user.name); // TypeScript knows it's a string!`,
    title: 'With BAML',
  },
];

export default function WhyPage() {
  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <Navbar />
      <main className="flex flex-col items-center justify-center divide-y divide-border min-h-screen w-full">
        {/* Hero Section */}
        <section className="relative flex w-full items-center justify-center px-4 py-20 md:py-32">
          <div className="flex flex-col items-center justify-center space-y-8 text-center">
            <h1 className="text-4xl font-bold tracking-tight sm:text-5xl md:text-6xl lg:text-7xl max-w-4xl">
              Why The F*** Did We Make a Language?
            </h1>
            <p className="max-w-2xl text-lg text-muted-foreground">
              BAML is the first language designed with LLMs in mind—cognitive
              functions that make AI as reliable as the rest of your stack.
              Because building AI applications shouldn't feel like defusing a
              bomb. Here's the story of why we built it and why you'll love it.
            </p>
            <div className="flex gap-4">
              <Button asChild>
                <Link href="/blog/why-baml">Read the Full Story</Link>
              </Button>
              <Button asChild variant="outline">
                <Link
                  className="flex items-center gap-2"
                  href="https://www.youtube.com/watch?v=baml-intro"
                >
                  <Play className="h-4 w-4" />
                  Watch Video
                </Link>
              </Button>
            </div>
          </div>
        </section>

        {/* The Problem Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="text-center mb-12">
              <h2 className="text-3xl font-bold mb-4">
                The Problem We All Face
              </h2>
              <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
                Every developer building AI applications runs into the same
                walls. We got tired of banging our heads against them.
              </p>
            </div>
            <div className="grid md:grid-cols-2 gap-6">
              {problems.map((item) => (
                <Card className="p-6" key={item.title}>
                  <div className="flex items-start gap-4">
                    <div className="h-10 w-10 rounded-lg bg-destructive/10 flex items-center justify-center flex-shrink-0">
                      <item.icon className="h-5 w-5 text-destructive" />
                    </div>
                    <div>
                      <h3 className="text-xl font-semibold mb-2">
                        {item.title}
                      </h3>
                      <p className="text-sm text-muted-foreground mb-3">
                        <span className="font-medium">Problem:</span>{' '}
                        {item.problem}
                      </p>
                      <p className="text-sm text-primary">
                        <span className="font-medium">Solution:</span>{' '}
                        {item.solution}
                      </p>
                    </div>
                  </div>
                </Card>
              ))}
            </div>
          </div>
        </section>

        {/* Code Comparison Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <h2 className="text-3xl font-bold text-center mb-12">
              See The Difference
            </h2>
            <div className="grid lg:grid-cols-2 gap-8">
              {comparisons.map((comp) => (
                <Card className="overflow-hidden" key={comp.title}>
                  <div
                    className={`px-6 py-4 ${comp.title === 'Without BAML' ? 'bg-destructive/5' : 'bg-primary/5'}`}
                  >
                    <h3 className="font-semibold text-lg">{comp.title}</h3>
                  </div>
                  <div className="p-6">
                    <pre className="text-sm bg-muted/50 p-4 rounded-md overflow-x-auto mb-4">
                      <code>{comp.code}</code>
                    </pre>
                    <ul className="space-y-2">
                      {comp.problems?.map((problem) => (
                        <li
                          className="flex items-center gap-2 text-sm"
                          key={problem}
                        >
                          <span className="text-destructive">✗</span>
                          <span className="text-muted-foreground">
                            {problem}
                          </span>
                        </li>
                      ))}
                      {comp.benefits?.map((benefit) => (
                        <li
                          className="flex items-center gap-2 text-sm"
                          key={benefit}
                        >
                          <span className="text-primary">✓</span>
                          <span className="text-muted-foreground">
                            {benefit}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </div>
                </Card>
              ))}
            </div>
          </div>
        </section>

        {/* The Journey Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="max-w-3xl mx-auto">
              <h2 className="text-3xl font-bold mb-8">Our Journey</h2>
              <div className="prose prose-lg dark:prose-invert">
                <p className="text-lg text-muted-foreground mb-6">
                  We started building AI applications in 2022, right when
                  ChatGPT launched. Like everyone else, we were excited about
                  the possibilities. But the reality hit hard.
                </p>
                <p className="text-lg text-muted-foreground mb-6">
                  <span className="font-semibold text-foreground">
                    Month 1:
                  </span>{' '}
                  "This is amazing! We can build anything!" String concatenation
                  everywhere. JSON.parse() on every response. Ship it! 🚀
                </p>
                <p className="text-lg text-muted-foreground mb-6">
                  <span className="font-semibold text-foreground">
                    Month 2:
                  </span>{' '}
                  Production errors. Everywhere. The AI returns slightly
                  different formats. Our parsing breaks. Customers are angry. We
                  add try-catch blocks around everything.
                </p>
                <p className="text-lg text-muted-foreground mb-6">
                  <span className="font-semibold text-foreground">
                    Month 3:
                  </span>{' '}
                  We build a validation layer. Then a type generation system.
                  Then a prompt management system. We're building more
                  infrastructure than actual features.
                </p>
                <p className="text-lg text-muted-foreground mb-6">
                  <span className="font-semibold text-foreground">
                    Month 4:
                  </span>{' '}
                  F*** it. If we're going to solve this problem, let's solve it
                  properly. For everyone.
                </p>
                <p className="text-lg font-semibold text-foreground">
                  And that's how BAML was born.
                </p>
              </div>
            </div>
          </div>
        </section>

        {/* Why a Language Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="grid lg:grid-cols-2 gap-12 items-center">
              <div>
                <h2 className="text-3xl font-bold mb-6">
                  Why a Language Though?
                </h2>
                <div className="space-y-4">
                  <div>
                    <h3 className="text-xl font-semibold mb-2 flex items-center gap-2">
                      <Zap className="h-5 w-5 text-primary" />
                      Purpose-Built for AI
                    </h3>
                    <p className="text-muted-foreground">
                      General-purpose languages weren't designed for AI
                      interactions. BAML is built specifically for defining AI
                      interfaces, with first-class support for prompts, schemas,
                      and validations.
                    </p>
                  </div>
                  <div>
                    <h3 className="text-xl font-semibold mb-2 flex items-center gap-2">
                      <Shield className="h-5 w-5 text-primary" />
                      Compile-Time Safety
                    </h3>
                    <p className="text-muted-foreground">
                      Catch errors before they hit production. BAML's compiler
                      validates your schemas, prompts, and type definitions at
                      build time, not runtime.
                    </p>
                  </div>
                  <div>
                    <h3 className="text-xl font-semibold mb-2 flex items-center gap-2">
                      <Rocket className="h-5 w-5 text-primary" />
                      Developer Experience
                    </h3>
                    <p className="text-muted-foreground">
                      Full IDE support, automatic code generation, and seamless
                      integration with your existing TypeScript/Python codebase.
                      It just works.
                    </p>
                  </div>
                </div>
              </div>
              <div className="bg-muted/20 rounded-lg p-8 border border-border">
                <h3 className="font-semibold mb-4">
                  What developers are saying:
                </h3>
                <div className="space-y-4">
                  <Card className="p-4">
                    <p className="text-sm italic mb-2">
                      "Finally, someone who gets it. BAML is what I've been
                      trying to build internally for months."
                    </p>
                    <p className="text-xs text-muted-foreground">
                      - Senior Engineer at Fortune 500
                    </p>
                  </Card>
                  <Card className="p-4">
                    <p className="text-sm italic mb-2">
                      "Reduced our AI-related bugs by 90%. Not exaggerating."
                    </p>
                    <p className="text-xs text-muted-foreground">
                      - CTO at AI Startup
                    </p>
                  </Card>
                  <Card className="p-4">
                    <p className="text-sm italic mb-2">
                      "The VS Code extension alone is worth it. Pure developer
                      happiness."
                    </p>
                    <p className="text-xs text-muted-foreground">
                      - Full Stack Developer
                    </p>
                  </Card>
                </div>
              </div>
            </div>
          </div>
        </section>

        {/* CTA Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-4xl text-center">
            <h2 className="text-3xl font-bold mb-4">
              Ready to Stop Fighting Your Tools?
            </h2>
            <p className="text-lg text-muted-foreground mb-8 max-w-2xl mx-auto">
              Join thousands of developers who are building AI applications the
              right way. With type safety, reliability, and actually enjoyable
              developer experience.
            </p>
            <div className="flex flex-col sm:flex-row gap-4 justify-center">
              <Button asChild size="lg">
                <Link href="/play">Try in Playground</Link>
              </Button>
              <Button asChild size="lg" variant="outline">
                <Link href="/docs/getting-started">Get Started</Link>
              </Button>
            </div>
            <p className="text-sm text-muted-foreground mt-6">
              Still skeptical? That's healthy. Check out our{' '}
              <Link
                className="underline hover:text-primary"
                href="/docs/comparisons"
              >
                comparisons with alternatives
              </Link>
              .
            </p>
          </div>
        </section>

        <FooterSection />
      </main>
    </div>
  );
}
