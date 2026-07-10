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

const LANGUAGE_SNIPPET = `// One file describes the model boundary, the types,
// the provider, and the behavior around it.
class ToolCall {
  intent "answer" | "read_file" | "run_bash"
  confidence float
  args map<string, string>
}

function PickTool(history: string[]) -> ToolCall {
  client GPT4o
  prompt #"
    Choose the next tool for this agent loop.
    {{ ctx.output_format }}
    {{ _.role("user") }} {{ history }}
  "#
}

function should_continue(call: ToolCall) -> bool {
  call.intent != "answer" && call.confidence > 0.7
}

test "low-confidence tool call stops" {
  let call = ToolCall {
    intent "run_bash"
    confidence 0.4
    args {}
  }
  assert.eq(should_continue(call), false)
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
          maxHeight: 340,
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

function stableKeyPart(value: string) {
  let hash = 0;
  for (let i = 0; i < value.length; i += 1) {
    hash = (hash * 31 + value.charCodeAt(i)) >>> 0;
  }
  return hash.toString(36);
}

function highlightSyntax(code: string) {
  const seen = new Map<string, number>();
  const lines = code.split('\n');

  return lines.map((line, lineIndex) => {
    const count = seen.get(line) ?? 0;
    seen.set(line, count + 1);

    return (
      <span key={`line-${stableKeyPart(line)}-${count}`}>
        {highlightLine(line)}
        {lineIndex < lines.length - 1 ? '\n' : null}
      </span>
    );
  });
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
    /(\{\{[^}]+\}\}|#?"[^"]*"|`[^`]*`|\b[A-Z][A-Za-z0-9_]*\b|\b(?:class|function|client|provider|options|model|prompt|test|testset|let|return|if|else|for|match|type|in|is|string|int|bool|float|map|true|false)\b|\b(?:baml|ctx|assert|input|role)\b)/g,
  );

  const seen = new Map<string, number>();

  return pieces.map((piece) => {
    if (!piece) return null;
    const count = seen.get(piece) ?? 0;
    seen.set(piece, count + 1);

    let color = INK;
    if (/^\{\{/.test(piece)) color = PURPLE;
    else if (/^#?"/.test(piece) || /^`/.test(piece)) color = AMBER;
    else if (
      /^(class|function|client|provider|options|model|prompt|test|testset|let|return|if|else|for|match|type|in|is)$/.test(
        piece,
      )
    )
      color = PURPLE;
    else if (/^(string|int|bool|float|map|true|false)$/.test(piece))
      color = GREEN;
    else if (/^(baml|ctx|assert|input|role)$/.test(piece)) color = BLUE;
    else if (/^[A-Z]/.test(piece)) color = GREEN;

    return (
      <span key={`${stableKeyPart(piece)}-${count}`} style={{ color }}>
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
          Thesis · Boundary · 2026
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
            We want to make a language that agents are really good at writing,
            and humans are really good at understanding.
          </strong>{' '}
          The next wave of software will be edited by agents and reviewed by
          humans. That only works if the source is explicit, compact,
          compiler-checkable, and close to the model boundary it controls.
        </P>

        <UL>
          <li>
            Agents should edit one coherent program instead of chasing hidden
            prompts, JSON schemas, parser code, and client wrappers across a
            repo.
          </li>
          <li>
            Humans should be able to scan that same program and understand the
            types, prompts, providers, tests, and control flow without running
            the whole app in their head.
          </li>
          <li>
            The compiler should be the shared feedback loop for both of them:
            fast enough for agents to iterate, strict enough for people to
            trust.
          </li>
        </UL>

        <CodeBlock caption="Semantic describe gives agents the map before they edit.">
          {AGENT_SNIPPET}
        </CodeBlock>

        <P>
          <strong>
            BAML started at the model boundary because that is where today’s AI
            code breaks first.
          </strong>{' '}
          Prompts, schemas, parsing, retries, tests, and generated clients tend
          to drift apart as soon as a demo becomes a product. BAML puts those
          pieces in one file and gives them language semantics instead of
          framework conventions.
        </P>

        <CodeBlock caption="The boundary becomes a program agents and humans can both read.">
          {LANGUAGE_SNIPPET}
        </CodeBlock>

        <P>
          <strong>
            For v1, that foundation is growing into a Turing-complete language.
          </strong>{' '}
          Not just schemas. Typed functions, tagged unions, <Code>match</Code>,
          loops, tests, a standard library, and a VM. The goal is to let the
          ML-shaped part of an application become an actual program.
        </P>

        <CodeBlock caption="The ML-shaped part can become an actual program.">
          {V1_SNIPPET}
        </CodeBlock>

        <P>
          <strong>Tests have to live next to the behavior they protect.</strong>{' '}
          AI systems are probabilistic, but the product contract still needs to
          be checked. BAML test blocks make the expected behavior visible to
          developers, CI, and coding agents.
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
            Provider choice, deployment shape, and observability should be
            language-level concerns.
          </strong>{' '}
          If model calls are part of the program, switching providers, tracking
          cost and reliability, and deploying changes should not require a pile
          of disconnected glue code.
        </P>

        <CodeBlock caption="Provider choice should be configuration, not a rewrite.">
          {PROVIDER_SNIPPET}
        </CodeBlock>

        <P>
          <strong>
            The thesis is not that every AI app needs another framework.
          </strong>{' '}
          It is that agent-authored software needs a source format that is
          precise enough for machines to edit and legible enough for humans to
          own. BAML is our attempt to make that format a real programming
          language.
        </P>

        <UL>
          <li>
            Open source language, compiler, VM, LSP, formatter, and generated
            clients.
          </li>
          <li>
            Incremental adoption inside Python, TypeScript, Go, Ruby, and other
            existing systems.
          </li>
          <li>
            A path from typed model calls today to agent-readable programs
            tomorrow.
          </li>
        </UL>

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
            <Highlight>We don't have lock-in:</Highlight> the language,
            compiler, VM, and IDE integrations are all freely available and
            open-source, so teams can adopt the pieces they need.
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
