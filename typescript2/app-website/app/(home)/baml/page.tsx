import Link from 'next/link';
import type { ReactNode } from 'react';
import { createMetadata } from '@/app/_lib/metadata';
import { LINKS } from '@/components/home/ui';
import { LiveCode } from '@/components/playground/live-code';
import { BLOG_WRAP, BlogShell } from '../blog/_lib/shell';
import { SAMPLES, type Sample } from './_components/examples';

// CONTENT PARITY: keep the copy, the samples, and the commands in sync with content/baml.md.
// Update both in the same change.

// /baml: BAML introduced as a post, for a reader who has not heard of it and wants to know what it
// is, why it is a language, and whether its claims hold. It is one column in the blog's measure,
// read top to bottom, and every BAML sample in it is a live editor.
//
// Every number on this page is one the homepage already states (components/home/practical.tsx),
// and every command is one the quickstart documents.

const TITLE = 'BAML is a programming language with built-in telemetry';
const DESCRIPTION =
  'BAML is an open-source, statically typed language whose runtime records every function call. This post covers why we built a language to get that, what the language looks like, and what the tracing costs.';

export const metadata = createMetadata({
  description: DESCRIPTION,
  path: '/baml',
  title: TITLE,
  type: 'article',
});

/** A BAML sample, as a live editor. */
function Baml({ sample }: { sample: Sample }) {
  return (
    <LiveCode
      code={sample.code}
      filename={sample.file}
      runnable={sample.runnable}
    />
  );
}

/** Shell, SQL, or another language's code, as typed. */
function Listing({ children }: { children: ReactNode }) {
  return (
    <pre className="overflow-x-auto">
      <code>{children}</code>
    </pre>
  );
}

export default function Page() {
  return (
    <BlogShell>
      {/* The blog's measure, about 66 characters of body text, for the whole post. */}
      <article
        className={`${BLOG_WRAP} baml-post pt-10 pb-16 md:pt-14 md:pb-24`}
      >
        <div className="mx-auto max-w-[680px]">
          <header className="flex flex-col gap-4 border-b border-rule pb-8">
            <p className="flex flex-wrap items-center gap-x-2 gap-y-1 font-mono text-[13px] text-muted">
              <time dateTime="2026-11-17">November 17, 2026</time>
              <span aria-hidden>·</span>
              <span>The Boundary team</span>
            </p>
            <h1 className="font-sans text-[30px] font-semibold leading-[1.15] tracking-[-0.025em] text-head md:text-[38px]">
              {TITLE}
            </h1>
            <p className="text-[17px] leading-[1.6] text-body">
              It is open source, it is typed like TypeScript, and its runtime
              records every function call. This post covers why we built a
              language to get that, what the language looks like, and what the
              tracing costs.
            </p>
          </header>

          <div className="prose blog-prose pt-8">
            <h2 id="summary">The short version</h2>
            <ul>
              <li>
                BAML is a statically typed language with its own compiler and
                virtual machine. Both are written in Rust and licensed under
                Apache 2.0.
              </li>
              <li>
                The virtual machine records every function call: its arguments,
                its result, how long it took, and what called it. You don't
                install an SDK or annotate anything.
              </li>
              <li>
                The traces go to a SQLite file on your machine, or to Boundary
                Cloud if you use it, and you query them with SQL.
              </li>
              <li>
                You don't have to rewrite your application. BAML generates a
                typed SDK, so Python, TypeScript, Go, and other languages call
                BAML functions like their own.
              </li>
            </ul>
            <p>
              The BAML on this page is live. Each sample is an editor backed by
              the real compiler and virtual machine, built as WebAssembly and
              running in this tab, so nothing you type is sent to a server. Edit
              a sample and the compiler rechecks it. Press <strong>Run</strong>{' '}
              and it executes.
            </p>

            <h2 id="why">Why a language</h2>
            <p>
              We did not set out to build a programming language. We set out to
              make production software easier to debug, and kept reaching the
              same limit.
            </p>
            <p>
              Tracing is something you add. You install an SDK, wrap the calls
              you expect to matter, and decide what to attach to each span. The
              calls you didn't wrap leave no record, and during an incident
              those are often the ones you need.
            </p>
            <p>
              Coding agents make this worse in two ways. An agent instruments
              what it is told to, and not consistently. It is also the reader
              that most needs complete data, because it has no other way to
              learn what happened in production.
            </p>
            <p>
              A library sees the calls that are routed through it. A runtime
              sees all of them. So we wrote a runtime, and a runtime needs a
              language. Owning both also let us make choices that a library
              can't: types that still exist while the program runs, errors that
              are part of a function's type, and model calls that the compiler
              understands.
            </p>

            <h2 id="language">The language</h2>
            <p>
              The syntax will look familiar if you write TypeScript. The
              differences are in what the compiler checks and what survives to
              run time.
            </p>

            <h3 id="types">Types exist at run time</h3>
            <p>
              TypeScript erases its types before the program runs, so a value
              that arrives from the network is whatever it happens to be. In
              BAML a type is there at run time too. There is no <code>any</code>{' '}
              and no unchecked cast: to use an <code>unknown</code> as a{' '}
              <code>User</code>, you test it, and the test inspects the value.
            </p>
            <Baml sample={SAMPLES.narrowing} />
            <p>
              The same holds for a union. <code>match</code> picks an arm by the
              value's type, and the compiler checks that every member of the
              union has one. Delete an arm below to see the error.
            </p>
            <Baml sample={SAMPLES.match} />

            <h3 id="errors">Errors are part of the signature</h3>
            <p>
              A function declares what it can throw. <code>catch</code> matches
              on an error's type the way <code>match</code> matches on a
              value's, and the compiler checks that the arms cover what can be
              thrown. A function declared <code>throws never</code> has to
              handle everything, or it doesn't compile.
            </p>
            <Baml sample={SAMPLES.errors} />

            <h3 id="threads">Concurrency without async functions</h3>
            <p>
              <code>spawn</code> runs an expression on a green thread, and{' '}
              <code>await</code> joins it. Functions are not divided into
              synchronous and asynchronous kinds, so making one call concurrent
              doesn't change the signature of everything above it.
            </p>
            <Baml sample={SAMPLES.threads} />

            <h3 id="models">A model call is a function</h3>
            <p>
              Much of the code people write in BAML calls language models, so
              the language has a construct for it. A function can have a model,
              a prompt, and tools in place of a body. Its return type is the
              contract: the model's reply is parsed and checked against it
              before your code sees it. Tools are ordinary functions, and their
              doc comments are what the model reads.
            </p>
            <Baml sample={SAMPLES.agent} />
            <p>
              This one has no Run button, because calling the model needs an API
              key. It is still checked as you edit: change the return type and
              the compiler follows it through.
            </p>

            <h3 id="tests">Tests are part of the language</h3>
            <p>
              A test sits in the same file as the code it tests, and{' '}
              <code>baml test</code> runs it. A <code>testset</code> builds its
              tests when it runs, which is how you turn a file of production
              examples into one named test for each.
            </p>
            <Baml sample={SAMPLES.tests} />

            <h2 id="telemetry">What the runtime records</h2>
            <p>
              Every function call becomes a span: its name, arguments, return
              value, start time, duration, status, and parent. On your machine
              the spans are stored in SQLite, and <code>baml query</code> runs
              SQL over them.
            </p>
            <Listing>
              {`baml query "SELECT span_name, duration, input_args
            FROM spans
            ORDER BY duration DESC
            LIMIT 5"`}
            </Listing>
            <p>
              The arguments and results are stored as values with their types,
              not as strings, so a query can reach inside them:{' '}
              <code>WHERE input_args['customer']['name'] = 'bob'</code>.
            </p>
            <p>
              Recording every call is only practical if it is cheap. Observing
              one function call costs 32 nanoseconds in BAML. On the same
              machine, an Apple M2 Max, the same measurement with OpenTelemetry
              comes to 186 ns from Rust, 702 ns from TypeScript, and 9,288 ns
              from Python.
            </p>

            <h2 id="tools">The toolchain</h2>
            <p>
              One binary, <code>baml</code>, holds the compiler, the test
              runner, the formatter, and the tools below. They are built for
              coding agents as much as for people, which mostly means they are
              commands that print something an agent can read.
            </p>
            <ul>
              <li>
                <code>baml check</code> type-checks the project.
              </li>
              <li>
                <code>baml run greet -- --name "world"</code> runs any function
                as a command. Its parameters are the flags.
              </li>
              <li>
                <code>baml run -e '"a,b,c".split(",")'</code> evaluates an
                expression without a file.
              </li>
              <li>
                <code>baml describe</code> looks up a definition, its signature,
                what it depends on, and what refers to it.
              </li>
              <li>
                <code>baml test</code> runs the tests, and{' '}
                <code>baml query</code> queries the traces.
              </li>
            </ul>

            <h2 id="adopt">Using it from the code you have</h2>
            <p>
              You can write a whole service in BAML, but we recommend starting
              with one function: the part of the application you most need to
              watch. BAML generates a typed SDK for the host language, and the
              function is imported and called like any other.
            </p>
            <Listing>
              {`import { classify_ticket } from './baml_sdk';

const label = await classify_ticket(ticket);`}
            </Listing>
            <p>
              The call is traced like any other BAML call. There is a longer
              guide to <Link href="/adopt">the three ways to adopt BAML</Link>.
            </p>

            <h2 id="status">Status</h2>
            <p>
              BAML is pre-1.0. The language is usable today, and some APIs and
              tools will still change. These are not finished:
            </p>
            <ul>
              <li>
                <strong>Compiling to native code.</strong> Today BAML runs on
                its virtual machine. A native backend is{' '}
                <a href="https://github.com/BoundaryML/baml/pull/5218">
                  in progress
                </a>
                .
              </li>
              <li>
                <strong>Durable execution.</strong> Pausing a run on one machine
                and resuming it on another exists as{' '}
                <a href="https://github.com/BoundaryML/baml/pull/4956">
                  a prototype
                </a>
                .
              </li>
              <li>
                <strong>Sandboxes</strong> are{' '}
                <a href="https://beps.boundaryml.com/beps/63">proposed</a>,
                feature flags are being designed, and formal verification is
                research.
              </li>
            </ul>

            <h2 id="install">Install it</h2>
            <Listing>
              {`brew install baml
# or: curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s

baml init
baml run main`}
            </Listing>
            <p>
              The <Link href="/quickstart">quickstart</Link> has Windows and
              editor setup. The source is on <a href={LINKS.github}>GitHub</a>,
              and we answer questions in <a href={LINKS.discord}>Discord</a>.
            </p>
          </div>
        </div>
      </article>
    </BlogShell>
  );
}
