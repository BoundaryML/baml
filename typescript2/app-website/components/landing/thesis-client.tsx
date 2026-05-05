'use client';

const INK = '#1A1612';
const MUTED = '#5C5852';
const SOFT = '#8A8580';
const RULE = '#E5DFD0';
const MONO =
  '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
const BODY = `var(--font-geist-sans), "Helvetica Neue", Helvetica, Arial, sans-serif`;
const PURPLE = '#6D28D9';
const GREEN = '#047857';
const AMBER = '#B45309';
const BLUE = '#2563EB';
const COMMENT = '#8A8580';

const SCHEMA_SNIPPET = `// comments explain the schema to humans
// and are stripped before the LLM sees the prompt
class Ticket {
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

const PROVIDER_SNIPPET = `client GPT4o {
  provider openai
  options { model "gpt-4o" }
}

client Claude {
  provider anthropic
  options { model "claude-sonnet-4-5" }
}

// Change this one line to switch providers.
function TriageTicket(input: string) -> Ticket {
  client Claude
  prompt #"{{ ctx.output_format }} {{ input }}"#
}`;

const TEST_SNIPPET = `test "refund-is-high-priority" {
  TriageTicket("I was charged twice and need a refund today")
}

testset "triage-regression" {
  test "password-reset" {
    let ticket = TriageTicket("I cannot reset my password")
    assert.eq(ticket.priority, "medium")
  }
}`;

const V1_SNIPPET = `type Tool = Answer | ReadFile | RunBash

function dispatch(tool: Tool) -> string {
  match (tool) {
    a: Answer   => a.text,
    r: ReadFile => baml.fs.read(r.path),
    b: RunBash  => baml.sys.shell(b.command).stdout,
  }
}

function main() -> string {
  let history: string[] = [baml.io.input(">> ")]
  for (let _ = 0; _ < 5; _ += 1) {
    let step = PickTool(history)
    let result = dispatch(step.tool)
    if (step.tool is Answer) { return result }
    history.push(result)
  }
  "(turn limit)"
}`;

const AGENT_SNIPPET = `$ baml describe TriageTicket

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

function P({ children }: { children: React.ReactNode }) {
  return (
    <p
      style={{
        color: INK,
        fontFamily: BODY,
        fontSize: 15,
        lineHeight: 1.65,
        margin: '16px 0 0',
      }}
    >
      {children}
    </p>
  );
}

function UL({ children }: { children: React.ReactNode }) {
  return (
    <ul
      className="thesis-list"
      style={{
        color: INK,
        fontFamily: BODY,
        fontSize: 15,
        lineHeight: 1.55,
        listStylePosition: 'outside',
        listStyleType: 'disc',
        margin: '10px 0 0 0',
        paddingLeft: 22,
      }}
    >
      {children}
    </ul>
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

function Highlight({ children }: { children: React.ReactNode }) {
  return (
    <strong
      style={{
        color: INK,
        fontWeight: 700,
      }}
    >
      {children}
    </strong>
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
    <figure style={{ margin: '18px 0 28px' }}>
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
          maxHeight: 260,
          overflowX: 'auto',
          overflowY: 'auto',
          padding: '12px 14px',
        }}
      >
        <code style={{ fontFamily: MONO }}>{highlightSyntax(children)}</code>
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

function highlightSyntax(code: string) {
  return code.split('\n').map((line, lineIndex) => (
    <span key={`line-${lineIndex}`}>
      {highlightLine(line)}
      {lineIndex < code.split('\n').length - 1 ? '\n' : null}
    </span>
  ));
}

function highlightLine(line: string) {
  const commentStart = line.indexOf('//');
  const codePart = commentStart >= 0 ? line.slice(0, commentStart) : line;
  const commentPart = commentStart >= 0 ? line.slice(commentStart) : '';

  return (
    <>
      {highlightCodePart(codePart)}
      {commentPart ? (
        <span style={{ color: COMMENT }}>{commentPart}</span>
      ) : null}
    </>
  );
}

function highlightCodePart(part: string) {
  const pieces = part.split(
    /(\{\{[^}]+\}\}|#?"[^"]*"|`[^`]*`|\b[A-Z][A-Za-z0-9_]*\b|\b(?:class|function|client|provider|options|model|prompt|test|testset|let|return|if|for|match|type|in|is|string|int|bool|float)\b|\b(?:baml|ctx|assert|input)\b)/g,
  );

  return pieces.map((piece, index) => {
    if (!piece) return null;
    let color = INK;
    if (/^\{\{/.test(piece)) color = PURPLE;
    else if (/^#?"/.test(piece) || /^`/.test(piece)) color = AMBER;
    else if (
      /^(class|function|client|provider|options|model|prompt|test|testset|let|return|if|for|match|type|in|is)$/.test(
        piece,
      )
    )
      color = PURPLE;
    else if (/^(string|int|bool|float)$/.test(piece)) color = GREEN;
    else if (/^(baml|ctx|assert|input)$/.test(piece)) color = BLUE;
    else if (/^[A-Z]/.test(piece)) color = GREEN;

    return (
      <span key={`${piece}-${index}`} style={{ color }}>
        {piece}
      </span>
    );
  });
}

export function ThesisClient() {
  return (
    <main
      style={{
        background: '#ffffff',
        color: INK,
        fontFamily: BODY,
        padding: '56px 0 96px',
        width: '100%',
      }}
    >
      <style>
        {`
          .thesis-list li {
            margin: 4px 0;
            padding-left: 2px;
          }
          .thesis-list li::marker {
            color: ${SOFT};
          }
        `}
      </style>
      <article
        style={{
          margin: '0 auto',
          maxWidth: 680,
          padding: '0 24px',
        }}
      >
        <p
          style={{
            color: SOFT,
            fontFamily: MONO,
            fontSize: 11,
            letterSpacing: '0.14em',
            margin: 0,
            textTransform: 'uppercase',
          }}
        >
          Thesis · Boundary ML · 2026
        </p>

        <h1
          style={{
            color: INK,
            fontFamily: BODY,
            fontSize: 28,
            fontWeight: 600,
            letterSpacing: '-0.01em',
            lineHeight: 1.2,
            margin: '12px 0 0',
          }}
        >
          The BAML thesis
        </h1>

        <hr
          style={{
            border: 'none',
            borderTop: `1px solid ${RULE}`,
            margin: '36px 0 0',
          }}
        />

        <P>
          <strong>
            We want to make a language that agents are really good at writing.
          </strong>{' '}
          BAML files are small, explicit, and compiler-checkable. The prompt,
          schema, tests, provider, and generated client boundary are all in one
          place, so an agent does not have to infer the contract by chasing
          Python decorators, JSON schemas, hidden prompts, and parser code
          across a repo.
        </P>

        <UL>
          <li>
            Agents can edit one BAML function instead of four drift-prone files.
          </li>
          <li>
            The compiler gives fast feedback when a field, union arm, or return
            type is wrong.
          </li>
          <li>
            <Code>baml describe</Code> gives agents a semantic description of
            project and stdlib APIs before they guess.
          </li>
        </UL>

        <CodeBlock caption="Semantic describe gives agents the map before they edit.">
          {AGENT_SNIPPET}
        </CodeBlock>

        <P>
          <strong>
            We want to give people the right abstractions to build on top of
            their ML models:
          </strong>{' '}
          everything from inline comments that get stripped from your LLM
          prompts to support for Python and Typescript and making it easy to
          switch between ML service providers.
        </P>

        <CodeBlock caption="Schema, prompt, and output format live together.">
          {SCHEMA_SNIPPET}
        </CodeBlock>

        <P>
          <strong>
            We want to enable people to test the ML features and products
          </strong>{' '}
          that they're building, which is especially important when you're
          dealing with probabilistic systems and defining correctness is harder
          than enumerating edge cases!
        </P>

        <UL>
          <li>
            <Code>baml-cli test</Code> — runs test blocks against real providers
            or recorded fixtures, with <Code>@@assert</Code> constraints that
            fail CI on regression.
          </li>
        </UL>

        <CodeBlock caption="Tests live next to the ML function they protect.">
          {TEST_SNIPPET}
        </CodeBlock>

        <P>
          <strong>
            We want it to be easy to deploy changes to your ML features:
          </strong>{' '}
          you should be able to both self-host everything that calls an OpenAI
          API and ask us to handle that for you, function-as-a-service style.
        </P>

        <P>
          <strong>
            We want our users to be able to monitor their ML usage and ask
            questions
          </strong>{' '}
          about the precision and recall of their deployed model, about the
          costs of the current deployment, and about the reliability of the
          current deployment.
        </P>

        <P>
          <strong>
            We want it to be straightforward to refine your ML usage
          </strong>
          , whether that means LLM prompt tuning, fine-tuning an existing
          open-source model, or training a special-purpose model from scratch.
        </P>

        <CodeBlock caption="Provider choice should be configuration, not a rewrite.">
          {PROVIDER_SNIPPET}
        </CodeBlock>

        <P>And we think that the right way to do all this is to start with:</P>

        <UL>
          <li>
            A freely available, open-source schema language for your ML APIs,
          </li>
          <li>Code generation for your LLM interactions, and</li>
          <li>
            robust, fast, easy-to-use tooling to support every step of the
            process.
          </li>
        </UL>

        <P>
          For v1, that foundation is growing into a Turing-complete language:
          typed functions, tagged unions, <Code>match</Code>, loops, tests, a
          standard library, and a VM.
        </P>

        <CodeBlock caption="The ML-shaped part can become an actual program.">
          {V1_SNIPPET}
        </CodeBlock>

        <P>
          Importantly, this approach has a number of advantages compared to
          competitors in the space:
        </P>

        <UL>
          <li>
            <Highlight>
              We can offer our users a flexible, end-to-end platform.
            </Highlight>{' '}
            No one likes stitching together 10 products to build their workflow.
          </li>
          <li>
            <Highlight>We don't have lock-in:</Highlight> our schema language,
            compiler, and IDE integrations are all freely available and
            open-source, so if users want to use just those, they're more than
            welcome to.
          </li>
          <li>
            <Highlight>
              We can build our platform and ecosystem incrementally.
            </Highlight>{' '}
            Every platform suffers from the critical mass challenge - that you
            have to build out an entire platform for using it to be attractive,
            and then get enough adoption to accrue network effects - but
            everything that we want to build will be independently useful, so
            we'll be able to respond much more quickly to our users as we build
            out.
          </li>
          <li>
            <Highlight>We're not tied to LLMs:</Highlight> if the winds shift
            and the industry discovers new model architectures, hosting
            patterns, or whatnot, we'll be well-positioned to respond, because
            our value proposition is giving you the right abstractions for your
            ML APIs. We have a lot of special support for working with LLMs,
            because the existing general-purpose LLMs are wildly useful. But
            there's definitely some insanity to the fact that API calls in the
            LLM world can and do take multiple seconds.
          </li>
        </UL>

        <p
          style={{
            borderTop: `1px solid ${RULE}`,
            color: MUTED,
            fontFamily: BODY,
            fontSize: 13,
            lineHeight: 1.6,
            margin: '44px 0 0',
            paddingTop: 20,
          }}
        >
          Docs:{' '}
          <a
            href="https://docs.boundaryml.com"
            style={{
              color: '#7C3AED',
              textDecoration: 'underline',
              textUnderlineOffset: 2,
            }}
          >
            docs.boundaryml.com
          </a>
        </p>
      </article>
    </main>
  );
}
