import { Navbar } from '@/components/navbar';
import { FooterSection } from '@/components/footer-section';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import Link from 'next/link';
import {
  Bot,
  Workflow,
  FileCode,
  Shield,
  TestTube,
  LineChart,
  ArrowRight,
  CheckCircle,
} from 'lucide-react';

const solutions = [
  {
    id: 'agents',
    icon: Bot,
    title: 'AI Agents',
    description:
      'Build autonomous AI agents that can reason, plan, and execute complex tasks reliably',
    features: [
      'Type-safe agent definitions',
      'Multi-step reasoning chains',
      'State management and memory',
      'Tool integration framework',
    ],
    useCases: [
      'Customer support automation',
      'Code generation and review',
      'Research and analysis agents',
      'Task automation systems',
    ],
    caseStudy: {
      company: 'TechFlow Inc.',
      result:
        'Reduced support ticket resolution time by 75% with autonomous agents',
    },
  },
  {
    id: 'workflows',
    icon: Workflow,
    title: 'AI Workflows',
    description:
      'Orchestrate complex multi-step AI processes with guaranteed type safety at every step',
    features: [
      'Visual workflow builder',
      'Conditional branching',
      'Error handling and retries',
      'Parallel execution',
    ],
    useCases: [
      'Document processing pipelines',
      'Content generation workflows',
      'Data extraction and transformation',
      'Multi-model orchestration',
    ],
    caseStudy: {
      company: 'ContentCo',
      result:
        'Automated 90% of content production workflow with zero type errors',
    },
  },
  {
    id: 'prompt-management',
    icon: FileCode,
    title: 'Prompt Management',
    description:
      'Version control, test, and optimize your prompts with engineering best practices',
    features: [
      'Prompt versioning and rollback',
      'A/B testing framework',
      'Performance analytics',
      'Collaborative editing',
    ],
    useCases: [
      'Prompt optimization',
      'Multi-variant testing',
      'Cross-team collaboration',
      'Prompt library management',
    ],
    caseStudy: {
      company: 'AI Startup',
      result: 'Improved prompt performance by 40% through systematic testing',
    },
  },
  {
    id: 'production-reliability',
    icon: Shield,
    title: 'Production Reliability',
    description:
      'Deploy AI with confidence using enterprise-grade monitoring, testing, and safeguards',
    features: [
      'Runtime type validation',
      'Automatic fallbacks',
      'Rate limiting and quotas',
      'Comprehensive logging',
    ],
    useCases: [
      'Mission-critical AI systems',
      'Financial applications',
      'Healthcare AI',
      'Enterprise deployments',
    ],
    caseStudy: {
      company: 'FinanceApp',
      result:
        '99.9% uptime with automatic error recovery and zero data corruption',
    },
  },
  {
    id: 'proof-of-concepts',
    icon: TestTube,
    title: 'Rapid Prototyping',
    description:
      "Go from idea to working prototype in hours, not weeks, with BAML's development tools",
    features: [
      'Quick start templates',
      'Live playground testing',
      'Instant type generation',
      'One-click deployment',
    ],
    useCases: [
      'POC development',
      'Hackathon projects',
      'Client demos',
      'Feature validation',
    ],
    caseStudy: {
      company: 'Innovation Labs',
      result: 'Built and deployed 5 AI prototypes in a single sprint',
    },
  },
  {
    id: 'benchmarking',
    icon: LineChart,
    title: 'AI Benchmarking',
    description:
      'Measure and compare AI model performance with standardized testing frameworks',
    features: [
      'Automated test suites',
      'Performance metrics',
      'Model comparison tools',
      'Cost analysis',
    ],
    useCases: [
      'Model selection',
      'Performance optimization',
      'Cost-benefit analysis',
      'Regression testing',
    ],
    caseStudy: {
      company: 'Enterprise Co',
      result: 'Reduced AI costs by 60% through systematic benchmarking',
    },
  },
];

export default function SolutionsPage() {
  return (
    <div className="max-w-7xl mx-auto border-x relative">
      <Navbar />
      <main className="flex flex-col items-center justify-center divide-y divide-border min-h-screen w-full">
        {/* Hero Section */}
        <section className="relative flex w-full items-center justify-center px-4 py-20 md:py-32">
          <div className="flex flex-col items-center justify-center space-y-8 text-center">
            <h1 className="text-4xl font-bold tracking-tight sm:text-5xl md:text-6xl lg:text-7xl max-w-4xl">
              Solutions for Every AI Use Case
            </h1>
            <p className="max-w-2xl text-lg text-muted-foreground">
              From autonomous agents to production workflows, BAML—the language
              for AI—gives you the foundation for building reliable AI at any
              scale.
            </p>
            <div className="flex gap-4">
              <Button asChild>
                <Link href="/docs/solutions">Explore Solutions</Link>
              </Button>
              <Button variant="outline" asChild>
                <Link href="/play">Try Examples</Link>
              </Button>
            </div>
          </div>
        </section>

        {/* Solutions Grid */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="space-y-20">
              {solutions.map((solution, index) => (
                <div
                  key={solution.id}
                  className={`grid lg:grid-cols-2 gap-12 items-center ${
                    index % 2 === 1 ? 'lg:flex-row-reverse' : ''
                  }`}
                >
                  <div className={index % 2 === 1 ? 'lg:order-2' : ''}>
                    <div className="flex items-center gap-3 mb-6">
                      <div className="h-12 w-12 rounded-lg bg-primary/10 flex items-center justify-center">
                        <solution.icon className="h-6 w-6 text-primary" />
                      </div>
                      <h2 className="text-3xl font-bold">{solution.title}</h2>
                    </div>
                    <p className="text-lg text-muted-foreground mb-6">
                      {solution.description}
                    </p>

                    <div className="mb-8">
                      <h3 className="font-semibold mb-3">Key Features</h3>
                      <ul className="space-y-2">
                        {solution.features.map((feature) => (
                          <li
                            key={feature}
                            className="flex items-center gap-2 text-sm"
                          >
                            <CheckCircle className="h-4 w-4 text-primary" />
                            <span>{feature}</span>
                          </li>
                        ))}
                      </ul>
                    </div>

                    <Card className="p-4 mb-6 bg-primary/5 border-primary/20">
                      <p className="text-sm">
                        <span className="font-semibold">
                          {solution.caseStudy.company}:
                        </span>{' '}
                        {solution.caseStudy.result}
                      </p>
                    </Card>

                    <Button asChild className="group">
                      <Link href={`/docs/solutions/${solution.id}`}>
                        Learn More
                        <ArrowRight className="ml-2 h-4 w-4 transition-transform group-hover:translate-x-1" />
                      </Link>
                    </Button>
                  </div>

                  <div className={`${index % 2 === 1 ? 'lg:order-1' : ''}`}>
                    <Card className="p-8 bg-muted/20">
                      <h3 className="font-semibold mb-4">Common Use Cases</h3>
                      <div className="space-y-3">
                        {solution.useCases.map((useCase) => (
                          <div key={useCase} className="flex items-start gap-3">
                            <div className="h-2 w-2 rounded-full bg-primary mt-1.5 flex-shrink-0" />
                            <p className="text-sm text-muted-foreground">
                              {useCase}
                            </p>
                          </div>
                        ))}
                      </div>
                    </Card>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </section>

        {/* Integration Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <div className="text-center mb-12">
              <h2 className="text-3xl font-bold mb-4">Works With Your Stack</h2>
              <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
                BAML integrates seamlessly with your existing tools and AI
                providers
              </p>
            </div>
            <div className="grid md:grid-cols-4 gap-6">
              <Card className="p-6 text-center">
                <h3 className="font-semibold mb-2">AI Providers</h3>
                <p className="text-sm text-muted-foreground">
                  OpenAI, Anthropic, Google, Cohere, and more
                </p>
              </Card>
              <Card className="p-6 text-center">
                <h3 className="font-semibold mb-2">Languages</h3>
                <p className="text-sm text-muted-foreground">
                  TypeScript, Python, with more coming soon
                </p>
              </Card>
              <Card className="p-6 text-center">
                <h3 className="font-semibold mb-2">Frameworks</h3>
                <p className="text-sm text-muted-foreground">
                  Next.js, Express, FastAPI, Django, and more
                </p>
              </Card>
              <Card className="p-6 text-center">
                <h3 className="font-semibold mb-2">Infrastructure</h3>
                <p className="text-sm text-muted-foreground">
                  AWS, Azure, GCP, Vercel, Docker, K8s
                </p>
              </Card>
            </div>
          </div>
        </section>

        {/* Demo Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-6xl">
            <Card className="p-8 md:p-12 bg-primary/5 border-primary/20">
              <div className="text-center">
                <h2 className="text-3xl font-bold mb-4">See BAML in Action</h2>
                <p className="text-lg text-muted-foreground mb-8 max-w-2xl mx-auto">
                  Check out our demo repository with real-world examples of each
                  solution
                </p>
                <div className="flex flex-col sm:flex-row gap-4 justify-center">
                  <Button asChild>
                    <Link href="https://github.com/boundaryml/baml-demos">
                      View Demo Repository
                    </Link>
                  </Button>
                  <Button variant="outline" asChild>
                    <Link href="/play">Try in Playground</Link>
                  </Button>
                </div>
              </div>
            </Card>
          </div>
        </section>

        {/* CTA Section */}
        <section className="w-full px-4 py-20">
          <div className="mx-auto max-w-4xl text-center">
            <h2 className="text-3xl font-bold mb-4">
              Ready to Build Better AI?
            </h2>
            <p className="text-lg text-muted-foreground mb-8 max-w-2xl mx-auto">
              Start with our free tier and scale as you grow. No credit card
              required.
            </p>
            <div className="flex flex-col sm:flex-row gap-4 justify-center">
              <Button size="lg" asChild>
                <Link href="/docs/getting-started">Get Started Free</Link>
              </Button>
              <Button size="lg" variant="outline" asChild>
                <Link href="https://cal.com/boundaryml/30min">
                  Talk to an Expert
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
