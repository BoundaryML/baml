import { Globe } from 'lucide-react';
import { FirstBentoAnimation } from '@/components/first-bento-animation';
import { FourthBentoAnimation } from '@/components/fourth-bento-animation';
import { DeploymentStatus } from '@/components/magicui/deployment-status';
import { SyntaxTypingAnimation } from '@/components/magicui/syntax-typing-animation';
import { WordRotate } from '@/components/magicui/word-rotate';
import { SecondBentoAnimation } from '@/components/second-bento-animation';
import { ThirdBentoAnimation } from '@/components/third-bento-animation';
import { VSCodeMock } from '@/components/vscode';
import { cn } from '@/lib/utils';

// import { FirstBentoAnimation } from '@/todo/(marketing)/_components/first-bento-animation';
// import { FourthBentoAnimation } from '@/todo/(marketing)/_components/fourth-bento-animation';
// import { SecondBentoAnimation } from '@/todo/(marketing)/_components/second-bento-animation';
// import { SecurityShieldBackground } from '@/todo/(marketing)/_components/security-shield-background';
// import { ThirdBentoAnimation } from '@/todo/(marketing)/_components/third-bento-animation';
// import { FirstBentoAnimation } from '@/todo/(marketing)/_components/first-bento-animation';
// import { FourthBentoAnimation } from '@/todo/(marketing)/_components/fourth-bento-animation';
// import { SecondBentoAnimation } from '@/todo/(marketing)/_components/second-bento-animation';
// import { SecurityShieldBackground } from '@/todo/(marketing)/_components/security-shield-background';
// import { ThirdBentoAnimation } from '@/todo/(marketing)/_components/third-bento-animation';

export const Highlight = ({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) => {
  return (
    <span
      className={cn(
        'p-1 py-0.5 font-medium dark:font-semibold text-purple-600 dark:text-purple-400',
        className,
      )}
    >
      {children}
    </span>
  );
};

export const BLUR_FADE_DELAY = 0.15;

// Team pricing constants
export const TEAM_PRICING = {
  BASE_PRICE_MONTHLY: 25,
  BASE_PRICE_YEARLY: 20,
  DEFAULT_SEATS: 1,
  INCLUDED_SEATS: 1,
  MAX_SEATS: 50,
  PRICE_PER_SEAT_MONTHLY: 8,
  PRICE_PER_SEAT_YEARLY: 6,
} as const;

const url = process.env.NEXT_PUBLIC_APP_URL || 'http://localhost:3000';
const bamlCode = `class Analysis {
  issues string[]
}

function AnalyzeCodebase(code: string) -> Analysis {
  client openai/gpt-4o
  prompt "Analyze the codebase for: {code}"
}`;

const bamlCodePython = `from baml_client import b

result = b.AnalyzeCodebase("<html>...</html>")

print(result)
`;

const bamlCodeTypescript = `import { b } from "baml_client"

const result = b.AnalyzeCodebase("<html>...</html>")

console.log(result)
`;

const bamlCodeRuby = `require "baml_client"

result = b.AnalyzeCodebase("<html>...</html>")

print(result)
`;

const bamlCodeGo = `import "baml_client"

result := b.AnalyzeCodebase("<html>...</html>")

fmt.Println(result)
`;

const bamlCodeJava = `import com.boundaryml.baml.client.BamlClient;

BamlClient b = new BamlClient();

result = b.AnalyzeCodebase("<html>...</html>")

print(result)
`;

export const siteConfig = {
  benefits: [
    {
      id: 1,
      image: '/Device-6.png',
      text: 'Build AI applications with type safety and reliability.',
    },
    {
      id: 2,
      image: '/Device-7.png',
      text: 'Generate TypeScript types automatically from your BAML schemas.',
    },
    {
      id: 3,
      image: '/Device-8.png',
      text: 'Debug faster with compile-time error checking and runtime validation.',
    },
    {
      id: 4,
      image: '/Device-1.png',
      text: 'Collaborate with your team on AI interfaces seamlessly.',
    },
  ],
  bentoSection: {
    description:
      'Cognitive functions for AI: type-safe interfaces, generated types, and validation—built for the future of software.',
    items: [
      {
        content: <FirstBentoAnimation />,
        description:
          'Define AI interfaces with confidence. Write BAML schemas that generate TypeScript types automatically.',
        id: 1,
        title: 'Type-Safe AI Interfaces',
      },
      {
        content: <SecondBentoAnimation />,
        description:
          'Get type-safe, validated responses from any LLM. Support for JSON, XML, YAML, and more output formats.',
        id: 2,
        title: 'Structured Outputs',
      },
      {
        content: <ThirdBentoAnimation />,
        description:
          'Test your AI functions in CI/CD pipelines to ensure they are working as expected.',
        id: 3,
        title: 'Test Your AI Functions in CI/CD',
      },
      {
        content: <FourthBentoAnimation once={false} />,
        description:
          'Automatically retry failed requests and provide fallback responses when errors occur.',
        id: 4,
        title: 'Automatic Retry and Fallback',
      },
    ],
    title: 'Built for the Future of AI',
  },
  companyShowcase: {
    companyLogos: [
      {
        id: 1,
        logo: 'OpenAI',
        name: 'OpenAI',
      },
      {
        id: 2,
        logo: 'Anthropic',
        name: 'Anthropic',
      },
      {
        id: 3,
        logo: 'Google',
        name: 'Google',
      },
      {
        id: 4,
        logo: 'Meta',
        name: 'Meta',
      },
    ],
  },
  cta: 'Playground',
  ctaSection: {
    backgroundImage: '/agent-cta-background.png',
    button: {
      href: 'https://calendly.com/boundaryml/meeting-with-founders?back=1&month=2023-12&utm_source=marketing-site&utm_medium=cta-button',
      text: 'Book a meeting with us',
    },
    id: 'cta',
    subtext: 'Start with cognitive coding in minutes',
    title: 'Software that humans trust',
  },
  description:
    'BAML is the first language designed with LLMs in mind. Cognitive coding for AI—code that agents write, software that humans trust.',
  faqSection: {
    description:
      "Answers to common questions about BAML and its features. If you have any other questions, please don't hesitate to contact us.",
    faQitems: [
      {
        answer:
          'BAML is the first language designed with LLMs in mind. It enables cognitive coding: cognitive functions that encapsulate LLM capabilities in classic programming functions. Code that agents write. Software that humans trust. You get schema definition, TypeScript type generation, and runtime validation so AI fits the same toolchain as the rest of your stack.',
        id: 1,
        question: 'What is BAML?',
      },
      {
        answer:
          'BAML works by letting you define cognitive functions—AI interfaces as first-class language constructs. These compile to native code in your language of choice and generate TypeScript types and runtime validation, ensuring type safety and reliability throughout your AI application.',
        id: 2,
        question: 'How does BAML work?',
      },
      {
        answer:
          'Yes, BAML is designed with security in mind. Your AI schemas and API keys are protected, and the platform provides input validation and output sanitization to ensure secure AI interactions.',
        id: 3,
        question: 'Is my data secure?',
      },
      {
        answer:
          'Simply install the BAML VS Code extension and start writing .baml files. The extension provides syntax highlighting, autocomplete, and validation to help you write correct BAML schemas.',
        id: 4,
        question: 'How do I get started with BAML in VS Code?',
      },
      {
        answer:
          'Yes, the VS Code extension and core BAML functionality are completely free to use. You can define schemas, generate types, and build AI applications at no cost. Premium features are available with paid plans.',
        id: 5,
        question: 'Is BAML free to use?',
      },
      {
        answer:
          'BAML is perfect for building AI applications that integrate with OpenAI, Anthropic, and other AI providers. It provides type-safe interfaces and automatic validation to ensure your AI interactions are reliable.',
        id: 6,
        question: 'How can I integrate with AI providers?',
      },
      {
        answer:
          'BAML supports multiple AI providers including OpenAI, Anthropic, Google, and others. You can easily define schemas for different providers and switch between them while maintaining type safety.',
        id: 7,
        question: 'Which AI providers does BAML support?',
      },
      {
        answer:
          'Yes! BAML schemas can be shared across your entire team. When team members check out the code, they automatically get access to the same BAML configuration and generated types.',
        id: 8,
        question: 'Can my entire team use the same BAML schemas?',
      },
      {
        answer:
          'Create a baml.config.json file in your repository with your BAML schemas. When team members check out the code, they automatically get access to the same configuration and generated types.',
        id: 9,
        question: 'How do I share BAML configuration with my team?',
      },
    ],
    title: 'Frequently Asked Questions',
  },
  featureSection: {
    description:
      'From idea to production—define, test, call, and deploy cognitive functions.',
    items: [
      {
        content: (
          <div className="text-base lg:text-lg text-muted-foreground leading-relaxed">
            Yes, Cursor, Claude, already know BAML.
            <br />
            Yes, we made a whole VSCode extension for BAML.
          </div>
        ),
        id: 1,
        title: 'Define your functions',
        // component: (
        //   <VSCodeMock
        //     className="w-full"
        //     code={bamlCode}
        //     dark={false}
        //     filename="agent.baml"
        //     // height={650}
        //     language="TypeScript"
        //     lineNumbers
        //     showSidebar={false}
        //     showStatusBar={false}
        //   />
        // ),
        video: '/define-agents.mp4',
      },
      {
        content:
          'Do it in VSCode, or the editor of your choice. Or in CI/CD with baml-cli test',
        id: 2,
        title: 'Test your functions',
        video: '/test-agent.mp4',
      },
      {
        component: (
          <VSCodeMock
            className="w-full"
            dark={false}
            files={[
              // {
              //   code: bamlCode,
              //   filename: 'agent.baml',
              //   language: 'BAML',
              // },
              {
                code: bamlCodePython,
                filename: 'main.py',
                language: 'Python',
              },
              {
                code: bamlCodeTypescript,
                filename: 'main.ts',
                language: 'TypeScript',
              },
              {
                code: bamlCodeRuby,
                filename: 'main.rb',
                language: 'Ruby',
              },
              {
                code: bamlCodeGo,
                filename: 'main.go',
                language: 'Go',
              },
              // {
              //   code: bamlCodeJava,
              //   filename: 'Main.java',
              //   language: 'Java',
              // },
            ]}
            lineNumbers
            showSidebar={false}
            showStatusBar={false}
            showTerminal={true}
            terminalContent={
              <SyntaxTypingAnimation
                code={`{
  "endpoints": [
    "https://api.example.com/auth/login",
    "https://api.example.com/auth/logout"
  ]
}`}
                // className="text-green-400"
                delay={1000}
                duration={30}
              />
            }
            terminalHeight={200}
          />
        ),
        content: (
          <div>
            <pre>baml-cli generate</pre>{' '}
            <div className="flex flex-row gap-1 items-center">
              converts BAML functions to native functions in{' '}
              <WordRotate
                className="text-purple-600 dark:text-purple-400"
                words={['Python', 'TypeScript', 'Ruby', 'Go']}
              />
            </div>
          </div>
        ),
        id: 3,
        title: 'Call your functions from any language',
      },
      {
        component: (
          <div className="flex justify-center p-4">
            <DeploymentStatus autoStart={true} className="w-full max-w-sm" />
          </div>
        ),
        content: (
          <div>
            Do nothing special for BAML. Since BAML generates native code in
            your language of choice, you can use it in any way you want.
          </div>
        ),
        id: 4,
        title: 'Deploy your AI',
      },
      // {
      //   component: <IterateTestDemo />,
      //   content:
      //     'Test and iterate on your prompts directly within the editor, enabling rapid experimentation and debugging.',
      //   id: 4,
      //   title: '4. Iterate & Test',
      // },
    ],
    title: 'Complete workflow for cognitive coding',
  },
  footerLinks: [
    {
      links: [
        { id: 1, title: 'About Us', url: '/who-are-we' },
        {
          id: 2,
          title: 'Why BAML?',
          url: 'https://gloochat.notion.site/benefits-of-baml',
        },
        {
          id: 3,
          title: 'Privacy Policy',
          url: '/privacy-policy',
        },
        {
          id: 4,
          title: 'Terms of Service',
          url: '/tos',
        },
      ],
      title: 'Company',
    },
    // {
    //   links: [
    //     {
    //       id: 12,
    //       title: 'VS Code Extension',
    //       url: 'https://marketplace.visualstudio.com/items?itemName=boundaryml.baml',
    //     },
    //     {
    //       id: 13,
    //       title: 'JetBrains Plugin',
    //       url: 'https://plugins.jetbrains.com/plugin/24002-baml',
    //     },
    //     {
    //       id: 14,
    //       title: 'BAML CLI',
    //       url: '/docs/cli',
    //     },
    //   ],
    //   title: 'Products',
    // },
    {
      links: [
        {
          id: 16,
          title: 'Changelog',
          url: 'https://docs.boundaryml.com/changelog/changelog',
        },
        { id: 18, title: 'Docs', url: 'https://docs.boundaryml.com' },
        { id: 19, title: 'Jobs', url: '/jobs' },
      ],
      title: 'Resources',
    },
    // {
    //   links: [
    //     { id: 3, title: 'All Comparisons', url: '/comparisons' },
    //     { id: 4, title: 'vs LangChain', url: '/comparisons#langchain' },
    //     { id: 5, title: 'vs OpenAI SDK', url: '/comparisons#openai-sdk' },
    //     { id: 6, title: 'vs Anthropic SDK', url: '/comparisons#anthropic-sdk' },
    //     { id: 7, title: 'vs TypeScript', url: '/comparisons#typescript' },
    //     { id: 8, title: 'vs Zod', url: '/comparisons#zod' },
    //     {
    //       id: 9,
    //       title: 'vs Pydantic',
    //       url: '/comparisons#pydantic',
    //     },
    //     { id: 10, title: 'vs JSDoc', url: '/comparisons#jsdoc' },
    //     { id: 11, title: 'vs GraphQL', url: '/comparisons#graphql' },
    //   ],
    //   title: 'Compare',
    // },
    {
      links: [
        { id: 20, title: 'GitHub', url: 'https://github.com/boundaryml' },
        { id: 21, title: 'Twitter', url: 'https://twitter.com/boundaryml' },
        { id: 22, title: 'Discord', url: 'https://boundaryml.com/discord' },
        {
          id: 23,
          title: 'LinkedIn',
          url: 'https://linkedin.com/company/boundaryml',
        },
        { id: 24, title: 'YouTube', url: 'https://youtube.com/@boundaryml' },
      ],
      title: 'Social',
    },
  ],
  growthSection: {
    description:
      'Where type safety meets AI development—designed to protect your AI applications and empower your development.',
    items: [
      {
        // content: <SecurityShieldBackground />,
        description:
          'Build AI applications without leaving your editor. Keep your focus on code while maintaining type safety throughout your AI development process.',
        id: 1,
        title: 'Seamless Workflow Integration',
      },
      {
        content: (
          <div className="relative flex size-full max-w-lg items-center justify-center overflow-hidden [mask-image:linear-gradient(to_top,transparent,black_50%)] -translate-y-20">
            <Globe className="top-28" />
          </div>
        ),
        description:
          'Works with VS Code, JetBrains IDEs, Cursor, and other editors. Compatible with all major AI providers and development environments.',
        id: 2,
        title: 'Universal Compatibility',
      },
    ],
    title: 'Built for the Future of AI',
  },
  hero: {
    badge: 'Free During Beta',
    badgeIcon: <span>🔥</span>,
    badgeUrl:
      'https://boundaryml.com/docs/getting-started?utm_source=marketing-site&utm_medium=hero-cta',
    cta: {
      primary: {
        href: '/playground',
        text: 'Playground',
      },
      secondary: {
        href: '/docs',
        text: 'View Docs',
      },
    },
    description:
      'Cognitive coding: LLM capabilities as classic functions. Code that agents write. Software that humans trust.',
    title: 'The language for cognitive coding',
  },
  keywords: [
    'AI Development',
    'Type Safety',
    'TypeScript Generation',
    'Schema Validation',
  ],
  links: {
    discord: 'https://boundaryml.com/discord',
    email: 'hello@boundaryml.com',
    github: 'https://github.com/boundaryml',
    twitter: 'https://twitter.com/boundaryml',
  },
  name: 'BAML',
  nav: {
    links: [
      { href: '/', id: 1, name: 'Home' },
      { href: '/blog', id: 3, name: 'Blog' },
      { href: '/podcast', id: 4, name: 'Podcast' },
      { href: '/who-are-we', id: 5, name: 'Team' },
      { href: '/jobs', id: 6, name: 'Jobs' },
      // { href: '/play', id: 5, name: 'Playground' },
      // { href: '/solutions', id: 6, name: 'Solutions' },
      // { href: '/pricing', id: 7, name: 'Pricing' },
    ],
  },
  pricing: {
    description:
      'Start for free and upgrade as your team grows. No credit card required.',
    pricingItems: [
      {
        buttonColor: 'bg-accent text-primary',
        buttonText: 'Playground',
        description: 'Perfect for individual developers',
        features: [
          'CLI & Editor extension access',
          'Unlimited BAML schemas',
          'TypeScript type generation',
          'Basic schema validation',
          'Local development',
          'Single developer',
          'Open source',
          'Community support',
        ],
        href: '/docs/getting-started?utm_source=marketing-site&utm_medium=pricing-cta-free',
        isPopular: false,
        name: 'Free',
        period: 'month',
        price: '$0',
        yearlyPrice: '$0',
      },
      {
        betaFree: false,
        buttonColor: 'bg-secondary text-white',
        buttonText: 'Start Trial',
        description: 'Ideal for development teams',
        features: [
          'Advanced type generation',
          'Runtime validation',
          'Unlimited schemas',
          'Team collaboration',
          'Unlimited developers',
          'Private schemas',
          'Custom transformations',
          'Advanced validation',
          'Priority support',
          'Custom integrations',
          'Enterprise features',
        ],
        href: '/docs/getting-started?utm_source=marketing-site&utm_medium=pricing-cta-team',
        isPopular: true,
        name: 'Team',
        period: 'month',
        price: '$25',
        yearlyPrice: '$20',
      },
      {
        buttonColor: 'bg-primary text-primary-foreground',
        buttonText: 'Schedule a call',
        description: 'For large organizations with custom needs',
        features: [
          'On-premise deployment',
          'SSO & SAML integration',
          'Custom rate limits',
          'Audit logs & compliance',
          '99.9% uptime SLA',
          'Dedicated account manager',
          'Priority support',
          'Custom training & onboarding',
        ],
        href: 'https://cal.com/boundaryml/30min',
        isPopular: false,
        name: 'Enterprise',
        period: 'month',
        price: 'Custom',
        yearlyPrice: 'Custom',
      },
    ],
    title: 'Simple, transparent pricing',
  },
  storyPage: {
    mission:
      'We believe AI needs a language of its own—not strings and APIs, but cognitive functions that fit the way software is built. Code that agents write. Software that humans trust.',
    timeline: [
      {
        body: 'C and Unix gave us languages for the machine and the system.',
        era: '1970s–80s',
        label: 'C, Unix',
      },
      {
        body: 'Java and JavaScript gave us languages for the network and the web.',
        era: '1990s–2000s',
        label: 'Java, JavaScript',
      },
      {
        body: 'TypeScript gave us type safety at scale.',
        era: '2010s',
        label: 'TypeScript',
      },
      {
        body: 'Python became the language of AI — but it was never designed for it.',
        era: '2020s',
        label: 'Python',
      },
      {
        body: 'LLMs are a new kind of compute. We\u2019re still gluing them in with strings and hope. BAML is the first language designed with LLMs in mind.',
        era: 'Now',
        label: 'BAML',
      },
    ],
    title: 'Our story',
    whyItMatters:
      'Cognitive coding means LLM capabilities become first-class functions: same repo, same toolchain, same reliability you expect from the rest of your stack. That\u2019s how we get to software that humans trust.',
  },
  storySection: {
    ctaHref: '/#story',
    ctaText: 'Explore the timeline',
    teaser:
      'Every era of computing got a language that fit. C for the machine. Java and JavaScript for the network. TypeScript for scale. Now LLMs are a new kind of compute—and we\u2019re still gluing them in with strings. BAML is the first language designed with LLMs in mind.',
    timeline: [
      { era: '1970s–80s', label: 'C, Unix' },
      { era: '1990s–2000s', label: 'Java, JavaScript' },
      { era: '2010s', label: 'TypeScript' },
      { era: 'Now', label: 'BAML' },
    ],
    title: 'A language for the future of AI',
  },
  testimonials: [
    {
      description: (
        <p>
          BAML is amazing. I&apos;ve used it in Python and Typescript.{' '}
          <Highlight>It&apos;s a game changer.</Highlight>
        </p>
      ),
      id: '1',
      img: '/testimonials/people/user1.png',
      name: 'Adam Gitzes',
      role: 'Amazon',
    },
    {
      description: (
        <p>
          Just set up baml for my project, 10/10 experience and{' '}
          <Highlight>much faster than langchain</Highlight>.
        </p>
      ),
      id: '2',
      img: '/testimonials/people/jason.png',
      name: 'Jason Fan',
      role: 'Finic.ai',
    },
    {
      description: (
        <p>
          It&apos;s amazing!! Was able to{' '}
          <Highlight>
            cut down my tokens and time-to-first-token significantly without
            compromising results
          </Highlight>
          .
        </p>
      ),
      id: '3',
      img: '/testimonials/people/ray.png',
      name: 'Ray del Vecchio',
      role: 'Cerebral Valley',
    },
    {
      description: (
        <p>
          BAML is definitely a must have if you want any structured data from
          LLM; no more BS/long paragraphs describing what the output should be
          like, <Highlight>it just works!!!</Highlight>
        </p>
      ),
      id: '4',
      img: '/testimonials/people/hankel.png',
      name: 'Hankel Bao',
      role: 'Coldreach.ai',
    },
    {
      description: (
        <p>
          The test case and playground is quite literally the BEST feature. It
          has{' '}
          <Highlight>
            improved the iteration speed and quality by an order of magnitude
          </Highlight>
          .
        </p>
      ),
      id: '5',
      img: '/testimonials/people/joseph.png',
      name: 'Joseph Tutera',
      role: 'Docucare AI',
    },
    {
      description: (
        <p>
          I really really like what Baml offers [...] I think it&apos;s a
          step-wise improvement over Marvin. Having complete control over the
          prompt WITH strong type guarantees is fantastic.
          <br />I also think{' '}
          <Highlight>the dedicated testing playground is awesome.</Highlight>
        </p>
      ),
      id: '6',
      img: '/testimonials/people/gabe.png',
      name: 'Gabe',
      role: 'Zenfetch',
    },
    {
      description: (
        <p>
          Code is hella clean now. Look at [the] folder structure, and each
          folder for a respective pipeline. Each file just a prompt.{' '}
          <Highlight>Clean, elegant, beautiful.</Highlight>
        </p>
      ),
      id: '7',
      img: '/testimonials/people/paulo.png',
      name: 'Paulo Rossi',
      role: 'Magnaplay',
    },
    {
      description: (
        <p>
          Just got the categorizer to work first try.
          <br />
          Felt like landing a kickflip
        </p>
      ),
      id: '8',
      img: '/testimonials/people/eitan.png',
      name: 'Eitan Borgnia',
      role: 'Squack',
    },
  ],
  url,
};

export type SiteConfig = typeof siteConfig;
