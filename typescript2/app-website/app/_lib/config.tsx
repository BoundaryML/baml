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
      text: 'Build structured-output LLM boundaries with typed results.',
    },
    {
      id: 2,
      image: '/Device-7.png',
      text: 'Declare the return type once and let it drive parsing.',
    },
    {
      id: 3,
      image: '/Device-8.png',
      text: 'Use accurate compile diagnostics for prompt and type mismatches.',
    },
    {
      id: 4,
      image: '/Device-1.png',
      text: 'Keep tests, prompts, schemas, and clients in one .baml file.',
    },
  ],
  bentoSection: {
    description:
      'BAML compiles to bytecode for the Bex VM. The compiler is incremental like rust-analyzer. The stdlib is written in BAML. Built so agents can write it as fluently as they run in it.',
    items: [
      {
        content: <FirstBentoAnimation />,
        description:
          'TypeScript shaped surface. Optional chaining, null coalescing, struct literals, expression style if and match. LLMs author BAML at the speed they author TypeScript. Two hackathon teams shipped roughly 6,000 lines of valid BAML across nine independent agent projects.',
        id: 1,
        title: 'Agents write BAML fluently',
      },
      {
        content: <SecondBentoAnimation />,
        description:
          'Each tool is a class. The agent function returns a tagged union of those classes. match dispatches to handlers. No JSON schema bookkeeping. Adding a tool is one class plus one match arm. The agent loop is just a while loop in main().',
        id: 2,
        title: 'The agent is the program',
      },
      {
        content: <ThirdBentoAnimation />,
        description:
          'An LLM function is just a function with a named client and a Jinja prompt. The return type drives schema aware parsing. Four companions get synthesized for free: render_prompt, build_request, parse, parse_stream.',
        id: 3,
        title: 'First class LLM functions',
      },
      {
        content: <FourthBentoAnimation once={false} />,
        description:
          'testset blocks live next to the code they exercise. Generate tests at compile time with for loops. Run them in the playground or CI. 128 tests in 12 seconds is the kind of inner loop that lets an agent iterate fast.',
        id: 4,
        title: 'Inline tests as a language feature',
      },
    ],
    title: 'A programming language for agents',
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
    subtext:
      'Statically typed, Turing complete, with a custom VM and schema aware parsing. Agents write it. Agents run in it.',
    title: 'A language for agents.',
  },
  description:
    'BAML is a statically typed, Turing complete programming language with first class LLM functions, a custom VM, and schema aware parsing. Agents write BAML fluently and run in BAML cleanly.',
  faqSection: {
    description:
      "Answers to common questions about BAML and its features. If you have any other questions, please don't hesitate to contact us.",
    faQitems: [
      {
        answer:
          'BAML is a statically typed, Turing complete programming language with first class LLM functions, a custom VM (BexVM), an incremental query based compiler, namespaces, generics, lambdas, schema aware parsing, tagged union tool dispatch, and a stdlib that covers filesystem, HTTP, shell, env, and embeddings.',
        id: 1,
        question: 'What is BAML?',
      },
      {
        answer:
          'You write the whole workflow in one .baml file. Define classes for your data and tools. Write LLM functions with a named client and a Jinja prompt. Dispatch tools with match. Run testset blocks next to the code. Compile to bytecode. The agent is just a while loop calling a BAML function whose return type is a tagged union of tool classes.',
        id: 2,
        question: 'How does BAML work?',
      },
      {
        answer:
          'BAML keeps provider configuration in named client<llm> blocks and your application secrets in your normal runtime environment. The site and generated code do not require a custom database or hosted prompt store.',
        id: 3,
        question: 'How does BAML handle provider configuration?',
      },
      {
        answer:
          'Simply install the BAML VS Code extension and start writing .baml files. The extension provides syntax highlighting, autocomplete, and validation to help you write correct BAML schemas.',
        id: 4,
        question: 'How do I get started with BAML in VS Code?',
      },
      {
        answer:
          'Yes. The core language, generated clients, testset blocks, and local developer workflow are open source.',
        id: 5,
        question: 'Is BAML free to use?',
      },
      {
        answer:
          'BAML is strongest where the program shape and the agent shape line up: typed tool call dispatch, schema aware parsing of model output, structured errors, and inline tests, all in one source file. Multi stage LLM pipelines, tool calling agents, classification and extraction, typed deserialization, RAG over local data. The TypeScript shaped surface also means LLMs author BAML at roughly the speed they author TypeScript, with tight compile diagnostics for fast iteration. The cliff is steeper when you push into systems work like sockets, FFI, and bitwise ops, where the stdlib is still being built out.',
        id: 6,
        question: 'When does BAML shine?',
      },
      {
        answer:
          'Use named client<llm> blocks to configure providers and models, then reference that client from an LLM function body.',
        id: 7,
        question: 'Which AI providers does BAML support?',
      },
      {
        answer:
          'Yes. Put .baml files in baml_src/ and check them into the repo. The schema, prompt, testset blocks, and generated clients move with the codebase.',
        id: 8,
        question: 'Can my entire team use the same BAML schemas?',
      },
      {
        answer:
          'Use baml_src/ for project files. For larger projects, opt into namespaces with an ns_ directory prefix.',
        id: 9,
        question: 'How do I share BAML configuration with my team?',
      },
    ],
    title: 'Frequently Asked Questions',
  },
  featureSection: {
    description:
      'From idea to production: define, test, parse, and call first-class LLM functions.',
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
    title: 'Complete workflow for first-class LLM functions',
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
          url: '/changelog',
        },
        { id: 18, title: 'Docs', url: '/quickstart' },
        { id: 19, title: 'Jobs', url: '/jobs' },
        { id: 24, title: 'Blog', url: '/blog' },
        { id: 25, title: 'Pricing', url: '/pricing' },
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
      'Where typed prompt boundaries meet existing Python, TypeScript, and Ruby code.',
    items: [
      {
        // content: <SecurityShieldBackground />,
        description:
          'Write .baml files in your repo. Keep schema, prompt, and tests next to the code that calls them.',
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
          'Use generated clients from Python, TypeScript, Ruby, and Go while the .baml file remains the source of truth.',
        id: 2,
        title: 'Generated Clients',
      },
    ],
    title: 'Built for structured-output AI boundaries',
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
      'BAML supplies typed prompt boundaries for structured-output LLM work.',
    title: 'First-class LLM functions',
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
      { href: '/quickstart', id: 8, name: 'Quickstart' },
      { href: '/podcast', id: 4, name: 'Podcast' },
      { href: '/who-are-we', id: 5, name: 'Team' },
      // { href: '/play', id: 5, name: 'Playground' },
      // { href: '/pricing', id: 7, name: 'Pricing' },
    ],
  },
  pricing: {
    description:
      'Start with local .baml files and generated clients. Upgrade when your team needs shared workflows.',
    pricingItems: [
      {
        buttonColor: 'bg-accent text-primary',
        buttonText: 'Playground',
        description: 'Perfect for individual developers',
        features: [
          'CLI & Editor extension access',
          'Unlimited .baml files',
          'Generated Python, TypeScript, Ruby, and Go clients',
          'testset / test / assert.* blocks',
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
          'Generated clients',
          'Structured-output parsing',
          'Unlimited .baml files',
          'Team collaboration',
          'Unlimited developers',
          'Private projects',
          'Custom transformations',
          'Typed prompt boundaries',
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
    title: 'Pricing for typed LLM boundaries',
  },
  storyPage: {
    mission:
      'BAML is a statically-typed, expression-oriented language with first-class LLM functions: typed prompt boundaries that fit the way software is built.',
    timeline: [
      {
        body: 'C and Unix gave us languages for the machine and the system.',
        era: '1970s-80s',
        label: 'C, Unix',
      },
      {
        body: 'Java and JavaScript gave us languages for the network and the web.',
        era: '1990s-2000s',
        label: 'Java, JavaScript',
      },
      {
        body: 'TypeScript gave us type safety at scale.',
        era: '2010s',
        label: 'TypeScript',
      },
      {
        body: 'Python became the language of AI, but it was never designed for it.',
        era: '2020s',
        label: 'Python',
      },
      {
        body: 'LLMs are a new kind of compute. BAML supplies typed prompt boundaries for structured-output LLM work.',
        era: 'Now',
        label: 'BAML',
      },
    ],
    title: 'Our story',
    whyItMatters:
      'First-class LLM functions keep schema, prompt, model choice, tests, and generated clients in one place.',
  },
  storySection: {
    ctaHref: '/#story',
    ctaText: 'Explore the timeline',
    teaser:
      'Every era of computing got a language that fit. C for the machine. Java and JavaScript for the network. TypeScript for scale. Now LLMs need typed prompt boundaries and first-class LLM functions.',
    timeline: [
      { era: '1970s-80s', label: 'C, Unix' },
      { era: '1990s-2000s', label: 'Java, JavaScript' },
      { era: '2010s', label: 'TypeScript' },
      { era: 'Now', label: 'BAML' },
    ],
    title: 'A language for structured-output AI boundaries',
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
