'use client';

const INK = '#1A1612';
const MUTED = '#5C5852';
const SOFT = '#8A8580';
const RULE = '#E5DFD0';
const MONO =
  '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';
const BODY = `var(--font-geist-sans), "Helvetica Neue", Helvetica, Arial, sans-serif`;
const PURPLE = '#6D28D9';

const ATB_PIPELINE = `trigger (cron / @bammy / bug report)
  │
  ├─ task            a hard, generated benchmark task, solved in pure BAML
  │
  ├─ agent run       Claude Code + latest canary baml on PATH
  │                  (warm start: skill docs injected · cold start: nothing installed)
  │
  ├─ trophy.json     the agent's verbose self-report: every failed command,
  │                  every confusing error, doc gaps, verified minimal repros
  │
  ├─ dedup           LLM merges findings across runs -> stable issue list
  │                  (classified: skill issue vs language issue)
  │
  └─ fix loop        issues mirror to Linear -> human approves ->
                     a fix agent is dispatched -> PR -> CI -> merge`;

const ADHERENCE_PIPELINE = `codebase
  │  (a) chunk via compiler AST          · deterministic
  ├─ chunk table
  │  (b) build interaction graph          · deterministic
  ├─ interaction table
  │  (c) infer intention per chunk        · LLM, graph-grounded
  ├─ intention table
  │  (d) route principles -> chunks       · static prefilter + semantic gate
  ├─ (chunk, principle) worklist
  │  (e) grade adherence 1-10             · LLM w/ anchored rubric
  │  (f) adversarially verify low grades  · LLM skeptics
  ├─ finding list
  │  (g) codebase-level omission scan     · "never reached for the primitive"
  └─ (h) aggregate -> score + slop report`;

const INTENTION_ROW = `chunk           intention (goal)                        mechanism (how)
─────────────   ─────────────────────────────────────   ──────────────────────────
parse_date_str  turn model output into a comparable     manual string splitting
                date                                    on "-"

// goal: legitimate. mechanism: ignores the time stdlib the
// language shipped for exactly this. the gap is where slop lives.`;

const GRADING_SCALE = `9-10  adherent     uses the primitive as the BEP intends;
                   reads like a BEP usage example
7-8   adherent     right primitive, minor deviation from intended form
5-6   neutral      principle applies but the code neither
                   exploits nor fights it
3-4   fighting     works around the primitive: stringly-typed data
                   past the type system, catch-and-ignore around
                   error design, prompt-string concatenation
1-2   reinventing  reimplements a primitive by hand: manual JSON
                   parsing, hand-rolled date math, ad-hoc retry loops`;

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
      className="benchmarking-list"
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

function OL({ children }: { children: React.ReactNode }) {
  return (
    <ol
      className="benchmarking-list"
      style={{
        color: INK,
        fontFamily: BODY,
        fontSize: 15,
        lineHeight: 1.55,
        listStylePosition: 'outside',
        listStyleType: 'decimal',
        margin: '10px 0 0 0',
        paddingLeft: 22,
      }}
    >
      {children}
    </ol>
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
  return <strong style={{ color: INK, fontWeight: 700 }}>{children}</strong>;
}

function Aside({ children }: { children: React.ReactNode }) {
  return (
    <aside
      style={{
        background: '#FBF8F0',
        border: `1px solid ${RULE}`,
        borderRadius: 5,
        display: 'flex',
        gap: 12,
        margin: '24px 0 0',
        padding: '16px 18px',
      }}
    >
      <span aria-hidden style={{ fontSize: 18, lineHeight: 1.4 }}>
        🐑
      </span>
      <div
        style={{
          color: INK,
          fontFamily: BODY,
          fontSize: 15,
          lineHeight: 1.6,
        }}
      >
        {children}
      </div>
    </aside>
  );
}

function H2({ children }: { children: React.ReactNode }) {
  return (
    <h2
      style={{
        color: INK,
        fontFamily: BODY,
        fontSize: 20,
        fontWeight: 600,
        letterSpacing: '-0.01em',
        lineHeight: 1.3,
        margin: '48px 0 0',
      }}
    >
      {children}
    </h2>
  );
}

function H3({ children }: { children: React.ReactNode }) {
  return (
    <h3
      style={{
        color: INK,
        fontFamily: BODY,
        fontSize: 16,
        fontWeight: 600,
        lineHeight: 1.35,
        margin: '32px 0 0',
      }}
    >
      {children}
    </h3>
  );
}

function Mono({ children, caption }: { children: string; caption: string }) {
  return (
    <figure style={{ margin: '18px 0 12px' }}>
      <pre
        style={{
          background: '#FBF8F0',
          border: `1px solid ${RULE}`,
          borderRadius: 5,
          color: INK,
          fontFamily: MONO,
          fontSize: 11,
          lineHeight: 1.55,
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

export function BenchmarkingClient() {
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
          .benchmarking-list li {
            margin: 4px 0;
            padding-left: 2px;
          }
          .benchmarking-list li::marker {
            color: ${SOFT};
          }
          .benchmarking-link {
            color: ${PURPLE};
            text-decoration: underline;
            text-underline-offset: 2px;
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
          Benchmarking · Boundary · 2026
        </p>

        <h1
          style={{
            color: INK,
            fontFamily: BODY,
            fontSize: 28,
            fontWeight: 600,
            letterSpacing: '-0.01em',
            lineHeight: 1.25,
            margin: '12px 0 0',
          }}
        >
          How we do benchmarking: testing BAML by testing agents on how well
          they can test BAML
        </h1>

        <hr
          style={{
            border: 'none',
            borderTop: `1px solid ${RULE}`,
            margin: '36px 0 0',
          }}
        />

        <H2>What is quality code?</H2>

        <Aside>
          At BAML, we have the unique ability to create a specific definition of
          what “high-quality” BAML code looks like, since we designed BAML. This
          gives us the insights needed to steer agents toward a quality BAML
          codebase. Humans tend to avoid writing quality code. Agents aren’t
          trained on quality code. The problem is the same.
        </Aside>

        <P>
          <Highlight>
            We propose that a quality codebase is a derivative of the user
            experience.
          </Highlight>{' '}
          Examining <em>how</em> code gets written is an excellent way to
          understand <em>why</em> certain codebases become slop. Are there weird
          loopholes? Is it hard to create abstractions? Is code readable? If the
          language makes the right thing awkward, both humans and agents route
          around it, and those workarounds accumulate into slop.
        </P>

        <P>
          So our benchmarking starts with the development process itself,
          instrumented from two directions: agents writing BAML, and humans
          writing BAML.
        </P>

        <H2>Agent Tries BAML</H2>

        <P>
          The principle of{' '}
          <a className="benchmarking-link" href="/atb">
            agent-tries-baml
          </a>{' '}
          is that we can{' '}
          <Highlight>co-design our language with agents</Highlight>; many of our
          decisions are <em>data driven</em>. We actively observe the agent
          developer experience and iterate on the language to make it better.
        </P>

        <P>
          Concretely, it’s an event-driven pipeline that repeatedly turns loose
          coding agents on hard benchmark tasks (algorithms to be solved in pure
          BAML, with the latest canary <Code>baml</Code> CLI on{' '}
          <Code>PATH</Code>) and treats every point of friction as a finding:
        </P>

        <Mono caption="The agent-tries-baml loop: agents as instrumented users of the language.">
          {ATB_PIPELINE}
        </Mono>

        <UL>
          <li>
            <Highlight>
              The task set is self-generating and deliberately hard.
            </Highlight>{' '}
            An LLM invents fresh algorithm tasks (exact signatures, edge cases,
            concrete input→output expectations) that exercise BAML’s
            general-purpose side: expression functions, classes, the stdlib,
            recursion, control flow.
          </li>
          <li>
            <Highlight>Self-reports are verified.</Highlight> Each run ends with
            a <Code>trophy.json</Code>: what worked, what failed, every non-zero{' '}
            <Code>baml</Code> command, every confusing message. Every language
            finding must include a minimal repro the agent actually ran, and
            every open bug is independently re-checked against each new nightly
            build.
          </li>
          <li>
            <Highlight>Cold starts are part of the benchmark.</Highlight> Some
            runs give the agent nothing installed at all, so the run also
            measures the onboarding and quickstart experience.
          </li>
          <li>
            <Highlight>Findings close the loop.</Highlight> Reports from many
            runs get deduplicated into a stable issue list, classified as
            skill-doc issues vs language issues, mirrored to our board, and,
            once a human approves, handed to a fix agent that opens a PR.
          </li>
        </UL>

        <H2>Human Tries BAML (aka bamlcode)</H2>

        <P>
          <a className="benchmarking-link" href="/bamlcode">
            bamlcode
          </a>{' '}
          is a LeetCode clone designed to test the <em>human</em> BAML
          experience: a small set of algorithms you can solve in the browser,
          plus a way to actively write feedback on the language while you’re in
          the middle of using it.
        </P>

        <P>
          We’ve noticed that{' '}
          <Highlight>humans and agents want different things</Highlight>. An
          agent will happily tolerate verbosity if the compiler errors are
          precise; a human wants ergonomics and forgiving syntax. Running the
          same tasks through both populations gives us a per-feature delta
          between what humans fight and what agents fight.
        </P>

        <P>
          With these two sources of data we can optimize the agent and human
          developer experience. Both measure the process of writing code; the
          next stage of the system grades the code itself.
        </P>

        <H2>The next stage: defining what quality code looks like</H2>

        <P>
          What makes things slop? Is it just understandability? Is it “taste”?
          How can we translate that feeling into a set of qualitative and
          quantitative stats to examine a BAML codebase? We already semi-achieve
          this by building certain principles into the language itself. But
          we’re in a unique position: having built BAML, we have a specific eye
          for what quality BAML code is. Simply, we can ask:
        </P>

        <Aside>
          Is this codebase using BAML’s primitives the way we designed them to
          be used? In other words: we track the <em>intentionality</em> of the
          decisions in a codebase.
        </Aside>

        <P>
          Every language feature we ship comes with a design document (a BEP)
          that records <em>why the feature exists and what it was for</em>,
          including the alternatives we rejected. Adherence to that recorded
          intent is our definition of quality. We distilled all 50 BEPs into a
          catalog of 144 <Highlight>principle cards</Highlight>: each one states
          the design intent, the shape adherent code takes, the concrete
          anti-patterns, and a routing predicate for which chunks of code it
          applies to.
        </P>

        <P>
          Each card is weighted by its BEP’s status. Implemented principles
          carry full weight; drafts are flagged but never scored; and rejected
          BEPs are <em>inverted</em>: the rejected design showing up in your
          code is itself the anti-pattern. 127 of the 144 cards carry scoring
          weight, and a card can only be violated by code that had the primitive
          available on its pinned toolchain.
        </P>

        <H3>The seven cross-cutting themes</H3>

        <P>
          Nearly every card instantiates one of seven themes. Together they are
          the language’s definition of quality, and the highest-level answer to
          “what is slop”:
        </P>

        <OL>
          <li>
            <Highlight>Derive the second artifact from the first.</Highlight>{' '}
            Slop = maintaining a hand-written parallel copy that can drift.
          </li>
          <li>
            <Highlight>One mechanism, with prescribed replacements.</Highlight>{' '}
            Slop = simulating an excluded feature with weaker parts.
          </li>
          <li>
            <Highlight>
              Policy lives at the caller/config layer, not inline.
            </Highlight>{' '}
            Slop = hand-rolled retry loops and per-function hardcoded policy.
          </li>
          <li>
            <Highlight>Purpose-built types over weak primitives.</Highlight>{' '}
            Slop = stringly-typed domain values and hand-rolled parsers.
          </li>
          <li>
            <Highlight>Explicit, visible intent at the point of use.</Highlight>{' '}
            Slop = ambient behavior and action at a distance.
          </li>
          <li>
            <Highlight>
              Strict by default; every relaxation is a named opt-in.
            </Highlight>{' '}
            Slop = broad catch-and-ignore and silent lossy defaults.
          </li>
          <li>
            <Highlight>Language-native evaluation and observability.</Highlight>{' '}
            Slop = eval criteria that exist only in prose.
          </li>
        </OL>

        <H2>Dynamic analysis: the adherence score</H2>

        <P>
          Using the principle catalog, we dynamically analyze BAML codebases for
          quality. The pipeline is a working, deterministic implementation,
          written in BAML and packed into a standalone CLI that takes a target
          project and writes a scored report. It avoids a single giant “review
          the repo” prompt, because that’s how you get vibes back out:
        </P>

        <Mono caption="The adherence pipeline: static analysis where possible, small parallel LLM judgments where not.">
          {ADHERENCE_PIPELINE}
        </Mono>

        <OL>
          <li>
            <Highlight>Chunk the codebase</Highlight> into functions, types,
            tests, and files, using the compiler’s AST for exact symbol
            boundaries. Chunks may overlap on purpose: some principles are about
            expressions, some about organization.
          </li>
          <li>
            <Highlight>Keep two tables.</Highlight> An interaction table of how
            chunks relate (<Code>calls</Code>, <Code>returns</Code>,{' '}
            <Code>contains</Code>, <Code>exercises</Code>…), because most misuse
            is only visible relationally. And an intention table: one inference
            per chunk of what the author is trying to achieve, and the mechanism
            they chose.
          </li>
          <li>
            <Highlight>Route principles to chunks</Highlight> with a static
            prefilter plus a semantic gate, producing a worklist of (chunk,
            principle) pairs.
          </li>
          <li>
            <Highlight>Grade each pair on an anchored 1–10 scale</Highlight>,
            requiring a quoted line of evidence for every grade. The quote is
            checked mechanically against the chunk source, and a grade whose
            evidence quote is missing from the source is neutralized.
          </li>
          <li>
            <Highlight>Aggregate</Highlight> into a weighted adherence score, a
            commission score (how well the code does where the design has an
            opinion), an omission score, a coverage stat, per-principle and
            per-file tables, and a slop report.
          </li>
        </OL>

        <H3>What we mean by intention</H3>

        <P>
          A short description of what the author (usually an agent) is intending
          to achieve with a decision, kept separate from the mechanism they
          chose to achieve it:
        </P>

        <Mono caption="One row of the intention table. The goal routes principles; the mechanism gets graded.">
          {INTENTION_ROW}
        </Mono>

        <H3>The grading scale is anchored, or the average is meaningless</H3>

        <Mono caption="Adherence grades, 1–10, per (chunk, principle) pair.">
          {GRADING_SCALE}
        </Mono>

        <P>
          Two extra passes keep the number honest. Every low grade gets an
          independent <Highlight>skeptic pass</Highlight>: a second judgment
          prompted to <em>refute</em> the finding (is there a legitimate reason
          this chunk can’t use the primitive?), because false accusations of
          slop destroy the metric’s credibility faster than missed slop does.
          And a codebase-level <Highlight>omission scan</Highlight> catches what
          chunk grading structurally misses: the codebase that never writes a
          test block, never streams, hand-rolls every enum as string constants.
          No chunk triggers those principles because the primitive never
          appears, and those are often the strongest slop signals.
        </P>

        <P>
          Runs are <Highlight>reproducible</Highlight>. The static stages are
          pure code, and every LLM judgment is stored in a content-addressed
          cache keyed by the prompt version, the stage, the model tier, and the
          exact inputs. A completed run replays byte-for-byte; re-running after
          an edit only re-judges the chunks that changed. Cheap, high-volume
          stages (intention and routing) run on a fast model tier while grading,
          refutation, and the omission scan run on a stronger one, and swapping
          either tier invalidates exactly the affected judgments.
        </P>

        <P>
          The headline number tracks the benchmark over time. The{' '}
          <Highlight>slop report</Highlight> carries the detail: each entry
          points to a line, names the design principle it ignores, and shows the
          intended form. That turns the score into a feedback loop for agents,
          for humans, and for the language design itself.
        </P>

        <H2>Closing the loop</H2>

        <P>
          Run the adherence pipeline on every agent-tries-baml solution and the
          score becomes a per-run metric next to pass/fail; diffs in
          per-principle scores across model versions show <em>which</em>{' '}
          primitives agents fight. Run it on bamlcode submissions and the
          human-vs-agent delta quantifies “humans and agents want different
          things.” And when everyone scores poorly on the same principle, we
          treat that as a design problem and change the language.
        </P>

        <P>
          Ideally, from this score, we demystify what “slop” is. Instead of some
          feeling of incompleteness, or an incomplete metric like lines of code,
          it becomes, simply:{' '}
          <Highlight>
            are we using the tools of a language the way they were intended to
            be used?
          </Highlight>
        </P>

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
          See it running:{' '}
          <a className="benchmarking-link" href="/atb">
            agent-tries-baml
          </a>{' '}
          ·{' '}
          <a className="benchmarking-link" href="/bamlcode">
            bamlcode
          </a>{' '}
          ·{' '}
          <a className="benchmarking-link" href="/thesis">
            the BAML thesis
          </a>
        </p>
      </article>
    </main>
  );
}
