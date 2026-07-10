import Link from 'next/link';
import { createMetadata } from '@/app/_lib/metadata';

export const metadata = createMetadata({
  description:
    'BAML for agents: why BAML exists, why agents are good at writing it, and the practical loop for editing BAML safely.',
  eyebrow: 'For Agents',
  path: '/agent',
  title: 'BAML for Agents',
});

const INK = '#1A1612';
const MUTED = '#5C5852';
const SOFT = '#8A8580';
const RULE = '#E5DFD0';
const ACCENT = '#7C3AED';
const MONO =
  'ui-monospace, SFMono-Regular, "JetBrains Mono", Menlo, Consolas, monospace';
const BODY =
  'var(--font-geist-sans), "Helvetica Neue", Helvetica, Arial, sans-serif';

const AGENT_LOOP = `baml run --list                 # compile and list callable functions
baml describe --symbols         # list project symbols
baml describe baml.json.decode  # inspect stdlib/project APIs before guessing
baml test --list
baml test -i "suite::case"
baml run --function Main --json-args @args.json --output json
baml fmt baml_src/main.baml
baml generate`;

const SEMANTIC_DESCRIBE = `$ baml describe TriageTicket

function TriageTicket(input: string) -> Ticket
client: GPT4o
input:
  input string
output:
  Ticket {
    priority "low" | "medium" | "high"
    summary string
  }
generated:
  TriageTicket$parse
  TriageTicket$render_prompt
  TriageTicket$build_request`;

const PROJECT_SHAPE = `my-app/
  baml.toml
  baml_src/
    main.baml
    types.baml
    clients.baml
    ns_eval/
      metrics.baml`;

const BAML_FUNCTION = `class Ticket {
  priority "low" | "medium" | "high"
  summary string
}

function TriageTicket(input: string) -> Ticket {
  client GPT4o
  prompt #"
    Classify this support ticket.
    {{ ctx.output_format }}

    Ticket: {{ input }}
  "#
}`;

const BAML_TEST = `test "refund-is-high-priority" {
  TriageTicket("I was charged twice and need a refund today")
}

testset "triage-regression" {
  test "password-reset" {
    let ticket = TriageTicket("I cannot reset my password")
    assert.eq(ticket.priority, "medium")
  }
}`;

const V1_PROGRAM = `type Tool = Answer | ReadFile | RunBash

function dispatch(tool: Tool) -> string {
  match (tool) {
    a: Answer   => a.text,
    r: ReadFile => baml.fs.read(r.path),
    b: RunBash  => baml.sys.shell(b.command).stdout,
  }
}`;

function P({ children }: { children: React.ReactNode }) {
  return (
    <p
      style={{
        color: INK,
        fontFamily: BODY,
        fontSize: 15,
        lineHeight: 1.65,
        margin: '14px 0 0',
      }}
    >
      {children}
    </p>
  );
}

function Code({ children }: { children: React.ReactNode }) {
  return (
    <code
      style={{
        background: '#F1ECDD',
        borderRadius: 3,
        color: INK,
        fontFamily: MONO,
        fontSize: '0.86em',
        padding: '1px 5px',
      }}
    >
      {children}
    </code>
  );
}

function CodeBlock({
  children,
  caption,
}: {
  children: string;
  caption: string;
}) {
  return (
    <figure style={{ margin: '18px 0 0' }}>
      <pre
        style={{
          background: '#FBF8F0',
          border: `1px solid ${RULE}`,
          borderRadius: 5,
          color: INK,
          fontFamily: MONO,
          fontSize: 11,
          lineHeight: 1.5,
          margin: 0,
          overflowX: 'auto',
          padding: '12px 14px',
        }}
      >
        <code style={{ fontFamily: MONO }}>{children}</code>
      </pre>
      <figcaption
        style={{
          color: SOFT,
          fontFamily: MONO,
          fontSize: 11,
          letterSpacing: '0.04em',
          marginTop: 6,
        }}
      >
        {caption}
      </figcaption>
    </figure>
  );
}

function Section({
  eyebrow,
  title,
  children,
}: {
  eyebrow: string;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section
      style={{
        borderTop: `1px solid ${RULE}`,
        marginTop: 40,
        paddingTop: 28,
      }}
    >
      <p
        style={{
          color: SOFT,
          fontFamily: MONO,
          fontSize: 11,
          letterSpacing: '0.12em',
          margin: 0,
          textTransform: 'uppercase',
        }}
      >
        {eyebrow}
      </p>
      <h2
        style={{
          color: INK,
          fontFamily: BODY,
          fontSize: 22,
          fontWeight: 650,
          letterSpacing: '-0.01em',
          lineHeight: 1.25,
          margin: '8px 0 0',
        }}
      >
        {title}
      </h2>
      {children}
    </section>
  );
}

function UL({ children }: { children: React.ReactNode }) {
  return (
    <ul
      style={{
        color: INK,
        fontFamily: BODY,
        fontSize: 15,
        lineHeight: 1.55,
        margin: '12px 0 0',
        paddingLeft: 22,
      }}
    >
      {children}
    </ul>
  );
}

export default function AgentPage() {
  return (
    <main
      style={{
        background: '#ffffff',
        color: INK,
        fontFamily: BODY,
        minHeight: '100vh',
        padding: '48px 24px 96px',
      }}
    >
      <article style={{ margin: '0 auto', maxWidth: 760 }}>
        <nav
          style={{
            alignItems: 'center',
            color: MUTED,
            display: 'flex',
            fontFamily: MONO,
            fontSize: 12,
            justifyContent: 'space-between',
            letterSpacing: '0.08em',
            marginBottom: 32,
            textTransform: 'uppercase',
          }}
        >
          <span>BAML for agents</span>
          <Link href="/" style={{ color: ACCENT, textDecoration: 'none' }}>
            Back to site
          </Link>
        </nav>

        <h1
          style={{
            color: INK,
            fontFamily: BODY,
            fontSize: 'clamp(34px, 6vw, 56px)',
            fontWeight: 700,
            letterSpacing: '-0.035em',
            lineHeight: 0.98,
            margin: 0,
            maxWidth: 700,
          }}
        >
          A language agents are really good at writing.
        </h1>

        <P>
          We want to make a language that agents are really good at writing.
          BAML files are small, explicit, and compiler-checkable. The prompt,
          schema, tests, provider, and generated client boundary are all in one
          place, so an agent does not have to infer the contract by chasing
          Python decorators, JSON schemas, hidden prompts, and parser code
          across a repo.
        </P>

        <CodeBlock caption="Semantic describe gives agents the map before they edit.">
          {SEMANTIC_DESCRIBE}
        </CodeBlock>

        <Section
          eyebrow="Thesis"
          title="The ML API should have a real contract."
        >
          <P>
            We want to give people the right abstractions to build on top of
            their ML models: everything from inline comments that get stripped
            from your LLM prompts to support for Python and Typescript and
            making it easy to switch between ML service providers.
          </P>
          <CodeBlock caption="Schema, prompt, and output format live together.">
            {BAML_FUNCTION}
          </CodeBlock>
          <P>
            We want to enable people to test the ML features and products that
            they're building, which is especially important when you're dealing
            with probabilistic systems and defining correctness is harder than
            enumerating edge cases.
          </P>
          <UL>
            <li>
              <Code>baml-cli test</Code> runs test blocks against real providers
              or recorded fixtures.
            </li>
            <li>
              <Code>@@assert</Code> constraints fail CI when an ML feature
              regresses.
            </li>
          </UL>
          <CodeBlock caption="Tests live next to the ML function they protect.">
            {BAML_TEST}
          </CodeBlock>
        </Section>

        <Section eyebrow="Guide" title="The edit loop for coding agents.">
          <P>
            Use the CLI as part of the edit loop. Do not invent stdlib names.
            Ask BAML what exists, keep the whole project compiling, format what
            you touch, and regenerate host-language clients when exported BAML
            types or functions change.
          </P>
          <CodeBlock caption="The minimum safe agent loop.">
            {AGENT_LOOP}
          </CodeBlock>
          <UL>
            <li>
              Run <Code>baml describe</Code> instead of guessing project or
              stdlib APIs.
            </li>
            <li>
              Use <Code>--json-args</Code> for classes, arrays, maps, optionals,
              unions, and nested input.
            </li>
            <li>
              Use <Code>--output json</Code> when a host program or bridge reads
              BAML output.
            </li>
            <li>
              Run <Code>baml generate</Code> after changing BAML functions or
              types that application code imports.
            </li>
          </UL>
        </Section>

        <Section eyebrow="Project shape" title="No imports. Predictable files.">
          <P>
            The CLI loads all <Code>.baml</Code> files under the project root or{' '}
            <Code>--from</Code> directory. Namespaces come from{' '}
            <Code>ns_*</Code> directories. Plain folders are just organization.
          </P>
          <CodeBlock caption="A project shape agents can inspect quickly.">
            {PROJECT_SHAPE}
          </CodeBlock>
          <UL>
            <li>
              Same-namespace symbols are bare. Cross-namespace symbols use names
              like <Code>root.eval.Score</Code>.
            </li>
            <li>
              Stdlib APIs use <Code>baml.*</Code>, <Code>assert.*</Code>,{' '}
              <Code>log.*</Code>, and <Code>io.*</Code>.
            </li>
            <li>Do not write import statements.</li>
          </UL>
        </Section>

        <Section
          eyebrow="v1"
          title="BAML is becoming a Turing-complete language."
        >
          <P>
            For v1, the schema/codegen foundation grows into a real language:
            typed functions, tagged unions, <Code>match</Code>, loops, tests, a
            standard library, and a VM. The ML-shaped part of your application
            can become an actual program.
          </P>
          <CodeBlock caption="Tools as types. Dispatch as exhaustive code.">
            {V1_PROGRAM}
          </CodeBlock>
        </Section>

        <Section eyebrow="Platform" title="Why this starts open-source.">
          <P>
            We can offer users a flexible, end-to-end platform. No one likes
            stitching together 10 products to build their workflow.
          </P>
          <P>
            We don't have lock-in: our schema language, compiler, and IDE
            integrations are all freely available and open-source, so if users
            want to use just those, they're more than welcome to.
          </P>
          <P>
            We can build the platform and ecosystem incrementally. Everything we
            want to build is independently useful, so we can respond quickly as
            users tell us what they actually need.
          </P>
          <P>
            We're not tied to LLMs. If the winds shift and the industry
            discovers new model architectures or hosting patterns, the value
            proposition is still the same: the right abstractions for your ML
            APIs.
          </P>
        </Section>

        <footer
          style={{
            borderTop: `1px solid ${RULE}`,
            color: MUTED,
            display: 'flex',
            flexWrap: 'wrap',
            fontFamily: MONO,
            fontSize: 12,
            gap: 16,
            marginTop: 48,
            paddingTop: 18,
          }}
        >
          <Link href="/agent/guide" style={{ color: ACCENT }}>
            Full BAML agent guide
          </Link>
          <a href="https://docs.boundaryml.com" style={{ color: ACCENT }}>
            Docs
          </a>
          <a
            href="https://github.com/BoundaryML/baml"
            style={{ color: ACCENT }}
          >
            GitHub
          </a>
        </footer>
      </article>
    </main>
  );
}
