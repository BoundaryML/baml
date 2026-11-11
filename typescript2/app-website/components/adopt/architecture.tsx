'use client';

import { type ReactNode, useEffect, useRef, useState } from 'react';
import {
  LangIcon,
  LangPicker,
  SDK_LANGS,
  usePinnedLang,
} from '@/components/home/lang-pick';
import { useReducedMotion } from '@/components/home/pinned';

// How BAML runs, for the /adopt page's last section and anyone who wants the detail: what
// `baml generate` makes, where the runtime lives in each target and language, and what happens at
// startup, when BAML throws, and at shutdown. Facts come from the SDK generators (sdks/*/sdkgen_*),
// the bridges (crates/bridge_cffi, sdks/*), sys_native and sys_wasm, and baml.panics.

/** Steps forward on a timer, looping, unless the reader prefers less motion. */
export function useBeat(count: number, ms: number) {
  const reduce = useReducedMotion();
  const [beat, setBeat] = useState(0);
  useEffect(() => {
    if (reduce) return;
    // A background tab doesn't advance: there's no one to see it.
    const timer = setInterval(() => {
      if (!document.hidden) setBeat((b) => (b + 1) % count);
    }, ms);
    return () => clearInterval(timer);
  }, [count, ms, reduce]);
  // Under reduced motion, the last beat: every diagram's finished, fully shown state.
  return reduce ? count - 1 : beat;
}

// ---------- I: the architecture, with things drawn inside the things they live in ----------

type SdkFile = { file: string; note: string; program?: boolean };
type Step = {
  note?: string;
  // Work that isn't repeated on later calls: once per process, or once per machine (a download
  // that's cached).
  once?: 'machine' | 'process';
  sysops?: boolean;
  text: string;
  tone?: 'baml' | 'bad' | 'host';
};
type Kind = 'bug' | 'expected' | 'panic' | 'unhandled';
type Case = { kind: Kind; outcome: string; when: string };

// How BAML lives inside an app in each language, in SDK_LANGS order. From the generators
// (sdks/*/sdkgen_*), the bridges (sdks/*, crates/bridge_cffi) and each SDK's runtime and exit hook.
// Every one loads the program through bridge_cffi's initialize_runtime_from_blob, which checks the
// toolchain version and starts the baml-worker threads.
const SHARED_LOAD: Step[] = [
  {
    once: 'process',
    text: 'Checks the toolchain version',
    tone: 'baml',
  },
  {
    once: 'process',
    text: 'Starts BAML’s worker threads',
    tone: 'baml',
  },
];
const SYSOPS_STEP: Step = {
  once: 'process',
  sysops: true,
  text: 'Sets up its sysops',
  tone: 'baml',
};
const BUG: Case = {
  kind: 'bug',
  outcome:
    'You get an SdkPanic, and your program keeps running. A crash during garbage collection still aborts.',
  when: 'The BAML runtime itself fails',
};
const drain = (hook: string, host: string, exits: string): Step[] => [
  { text: `${host} starts to exit`, tone: 'host' },
  { note: hook, text: 'BAML’s exit hook runs', tone: 'baml' },
  { text: 'Waits for calls and spawned tasks still running', tone: 'baml' },
  { text: 'Finishes writing recordings', tone: 'baml' },
  { text: exits, tone: 'host' },
];

const HOSTS: {
  callbacks: string;
  exceptions: Case[];
  files: SdkFile[];
  loads: string;
  process: string;
  runtime: string;
  shutdown: Step[];
  startup: Step[];
}[] = [
  {
    callbacks: 'your Python functions',
    exceptions: [
      {
        kind: 'expected',
        outcome: 'You get a normal Python exception. Catch it with try/except.',
        when: 'Your BAML function throws an error it declares',
      },
      {
        kind: 'panic',
        outcome:
          'You get a BamlPanic. It’s a BaseException, so a bare except Exception won’t swallow it by accident.',
        when: 'Your BAML code panics, like dividing by zero or a failed assert',
      },
      {
        kind: 'unhandled',
        outcome:
          'BAML prints it and exits, like any uncaught exception. Change that with set_unhandled_spawn_error_handler.',
        when: 'A background task throws, and nothing waits for it',
      },
      {
        ...BUG,
        outcome:
          'You get a BamlPanic carrying an SdkPanic, and your program keeps running. A crash during garbage collection still aborts.',
      },
    ],
    files: [
      { file: '__init__.py', note: 'your functions, typed' },
      { file: '__init__.pyi', note: 'types for your editor' },
      { file: '_inlinedbaml.py', note: 'your compiled program', program: true },
      { file: '_typemap.py', note: 'BAML types ↔ Python types' },
      { file: 'py.typed', note: 'marks the package as typed' },
    ],
    loads: 'when you import the SDK',
    process: 'your Python process',
    runtime: 'baml_bridge · native extension (pyo3)',
    shutdown: [
      { text: 'Python starts to exit', tone: 'host' },
      { note: 'atexit', text: 'BAML’s exit hook runs', tone: 'baml' },
      { text: 'Waits for calls and spawned tasks still running', tone: 'baml' },
      { text: 'Reports errors nothing observed', tone: 'baml' },
      { text: 'Finishes writing recordings', tone: 'baml' },
      {
        note: 'optional timeout cancels the rest',
        text: 'Python exits',
        tone: 'host',
      },
    ],
    startup: [
      { text: 'import baml_sdk', tone: 'host' },
      {
        once: 'process',
        text: 'Loads the native extension',
        tone: 'baml',
      },
      SYSOPS_STEP,
      {
        once: 'process',
        text: 'Decodes your compiled program',
        tone: 'baml',
      },
      ...SHARED_LOAD,
    ],
  },
  {
    callbacks: 'your JavaScript functions',
    exceptions: [
      {
        kind: 'expected',
        outcome: 'The Promise rejects. Catch it with try/catch.',
        when: 'Your BAML function throws an error it declares',
      },
      {
        kind: 'panic',
        outcome:
          'The Promise rejects with a BamlPanic. Catch it with try/catch.',
        when: 'Your BAML code panics, like dividing by zero or a failed assert',
      },
      {
        kind: 'unhandled',
        outcome:
          'It becomes an uncaught exception, and Node handles it like any other.',
        when: 'A background task throws, and nothing waits for it',
      },
      BUG,
    ],
    files: [
      { file: 'index.ts', note: 'your functions, typed' },
      { file: '<namespace>/index.ts', note: 'one module per namespace' },
      { file: '_inlinedbaml.ts', note: 'your compiled program', program: true },
      { file: '_typemap.ts', note: 'BAML types ↔ TypeScript types' },
      { file: '_invocation.ts', note: 'how calls are made' },
    ],
    loads: 'when you import the SDK',
    process: 'your Node process',
    runtime: '@boundaryml/baml-bridge · Node addon (napi)',
    shutdown: [
      { text: 'Node runs out of work', tone: 'host' },
      { note: 'beforeExit', text: 'BAML’s exit hook runs', tone: 'baml' },
      { text: 'Waits for calls and spawned tasks still running', tone: 'baml' },
      { text: 'Finishes writing recordings', tone: 'baml' },
      {
        note: 'process.exit() and signals skip the hook: call shutdownRuntime()',
        text: 'Node exits',
        tone: 'host',
      },
    ],
    startup: [
      { text: "import { classify_ticket } from './baml_sdk'", tone: 'host' },
      {
        once: 'process',
        text: 'Loads the Node addon',
        tone: 'baml',
      },
      SYSOPS_STEP,
      {
        once: 'process',
        text: 'Decodes your compiled program',
        tone: 'baml',
      },
      ...SHARED_LOAD,
    ],
  },
  {
    callbacks: 'your Go functions',
    exceptions: [
      {
        kind: 'expected',
        outcome: 'You get an err back.',
        when: 'Your BAML function throws an error it declares',
      },
      {
        kind: 'panic',
        outcome: 'You get an err back, like any other error.',
        when: 'Your BAML code panics, like dividing by zero or a failed assert',
      },
      {
        kind: 'unhandled',
        outcome:
          'It panics, which crashes your program. You can’t change that yet.',
        when: 'A background task throws, and nothing waits for it',
      },
      BUG,
    ],
    files: [
      { file: 'functions.go', note: 'your functions, typed' },
      { file: 'types.go', note: 'your types' },
      {
        file: 'internal/bootstrap/bootstrap.go',
        note: 'your compiled program',
        program: true,
      },
      { file: 'reflect/reflect.go', note: 'type information' },
    ],
    loads: 'on the first call',
    process: 'your Go program',
    runtime: 'baml-go · the BAML library, opened with dlopen',
    shutdown: [
      {
        note: 'Go has no exit hook',
        text: 'You call baml_go.Shutdown()',
        tone: 'host',
      },
      { text: 'Waits for calls and spawned tasks still running', tone: 'baml' },
      { text: 'Finishes writing recordings', tone: 'baml' },
      { text: 'main returns', tone: 'host' },
    ],
    startup: [
      {
        note: 'your first call',
        text: 'baml_sdk.ClassifyTicket(ctx, body)',
        tone: 'host',
      },
      {
        note: 'into ~/.baml/bridges',
        once: 'machine',
        text: 'Downloads the BAML library',
        tone: 'baml',
      },
      {
        once: 'process',
        text: 'Opens it',
        tone: 'baml',
      },
      SYSOPS_STEP,
      {
        once: 'process',
        text: 'Decodes your compiled program',
        tone: 'baml',
      },
      ...SHARED_LOAD,
    ],
  },
  {
    callbacks: 'your Rust functions',
    exceptions: [
      {
        kind: 'expected',
        outcome: 'You get an Err back.',
        when: 'Your BAML function throws an error it declares',
      },
      {
        kind: 'panic',
        outcome: 'You get an Err back.',
        when: 'Your BAML code panics, like dividing by zero or a failed assert',
      },
      {
        kind: 'unhandled',
        outcome: 'Nothing. The Rust SDK doesn’t report these yet.',
        when: 'A background task throws, and nothing waits for it',
      },
      BUG,
    ],
    files: [
      { file: 'src/lib.rs', note: 'your functions, typed' },
      {
        file: 'src/_inlinedbaml.bin',
        note: 'your compiled program',
        program: true,
      },
      { file: 'src/_runtime.rs', note: 'starts the runtime' },
      { file: 'Cargo.toml', note: 'a crate of its own' },
    ],
    loads: 'on the first call, or when you call init()',
    process: 'your Rust program',
    runtime: 'baml_bridge crate · loads the BAML library at run time',
    shutdown: drain('atexit', 'The program', 'The program exits'),
    startup: [
      {
        note: 'your first call',
        text: 'classify_ticket(body).await',
        tone: 'host',
      },
      {
        note: 'into ~/.baml/bridges',
        once: 'machine',
        text: 'Downloads the BAML library',
        tone: 'baml',
      },
      { once: 'process', text: 'Opens it', tone: 'baml' },
      SYSOPS_STEP,
      {
        once: 'process',
        text: 'Decodes your compiled program',
        tone: 'baml',
      },
      ...SHARED_LOAD,
    ],
  },
  {
    callbacks: 'your Java methods',
    exceptions: [
      {
        kind: 'expected',
        outcome:
          'It throws, or the CompletableFuture fails. Catch it either way.',
        when: 'Your BAML function throws an error it declares',
      },
      {
        kind: 'panic',
        outcome: 'It throws a BamlPanic.',
        when: 'Your BAML code panics, like dividing by zero or a failed assert',
      },
      {
        kind: 'unhandled',
        outcome:
          'It goes to the thread’s UncaughtExceptionHandler, like any Java exception.',
        when: 'A background task throws, and nothing waits for it',
      },
      BUG,
    ],
    files: [
      { file: 'Fns.java', note: 'your functions, typed' },
      { file: '<Type>.java', note: 'one per BAML type' },
      { file: 'Baml.java', note: 'starts the runtime' },
      { file: 'inlinedbaml.b64', note: 'your compiled program', program: true },
    ],
    loads: 'on the first call',
    process: 'your JVM',
    runtime: 'baml-bridge · JNI library, shipped in the jar',
    shutdown: drain('a JVM shutdown hook', 'The JVM', 'The JVM exits'),
    startup: [
      {
        note: 'your first call',
        text: 'Fns.classify_ticket(body)',
        tone: 'host',
      },
      {
        once: 'process',
        text: 'Unpacks the JNI library from the jar',
        tone: 'baml',
      },
      { once: 'process', text: 'Loads it', tone: 'baml' },
      SYSOPS_STEP,
      {
        once: 'process',
        text: 'Decodes your compiled program',
        tone: 'baml',
      },
      ...SHARED_LOAD,
    ],
  },
  {
    callbacks: 'your C++ functions',
    exceptions: [
      {
        kind: 'expected',
        outcome: 'It throws a C++ exception. Catch it with try/catch.',
        when: 'Your BAML function throws an error it declares',
      },
      {
        kind: 'panic',
        outcome: 'It throws a C++ exception.',
        when: 'Your BAML code panics, like dividing by zero or a failed assert',
      },
      {
        kind: 'unhandled',
        outcome:
          'It calls std::terminate, which ends your program. You can’t change that yet.',
        when: 'A background task throws, and nothing waits for it',
      },
      BUG,
    ],
    files: [
      { file: 'include/baml_sdk.h', note: 'your functions, typed' },
      { file: 'src/bindings.cc', note: 'how calls are made' },
      {
        file: 'src/_inlinedbaml.cc',
        note: 'your compiled program',
        program: true,
      },
      { file: 'CMakeLists.txt', note: 'builds it' },
    ],
    loads: 'on the first call',
    process: 'your C++ program',
    runtime: 'BAML headers · open the BAML library at run time',
    shutdown: drain('std::atexit', 'The program', 'The program exits'),
    startup: [
      {
        note: 'your first call',
        text: 'baml_sdk::classify_ticket(body)',
        tone: 'host',
      },
      {
        once: 'process',
        text: 'Finds the BAML library',
        tone: 'baml',
      },
      { once: 'process', text: 'Opens it', tone: 'baml' },
      SYSOPS_STEP,
      {
        once: 'process',
        text: 'Decodes your compiled program',
        tone: 'baml',
      },
      ...SHARED_LOAD,
    ],
  },
  {
    callbacks: 'your Swift functions',
    exceptions: [
      {
        kind: 'expected',
        outcome: 'It throws a Swift error. Catch it with do/catch.',
        when: 'Your BAML function throws an error it declares',
      },
      {
        kind: 'panic',
        outcome: 'It throws a BamlPanic.',
        when: 'Your BAML code panics, like dividing by zero or a failed assert',
      },
      {
        kind: 'unhandled',
        outcome:
          'It calls fatalError, which crashes your app. You can’t change that yet.',
        when: 'A background task throws, and nothing waits for it',
      },
      BUG,
    ],
    files: [
      { file: 'BamlRoot.swift', note: 'starts the runtime' },
      { file: '<Type>.swift', note: 'one per BAML type' },
      {
        file: '_InlinedBaml.swift',
        note: 'your compiled program',
        program: true,
      },
      { file: '_Invocation.swift', note: 'how calls are made' },
    ],
    loads: 'on the first call',
    process: 'your Swift app',
    runtime: 'BamlBridge · static library, linked when you build',
    shutdown: drain('atexit', 'The app', 'The app exits'),
    startup: [
      {
        note: 'your first call',
        text: 'try await classify_ticket(body: body)',
        tone: 'host',
      },
      {
        note: 'linked when you built',
        text: 'The runtime is already in the app',
        tone: 'baml',
      },
      SYSOPS_STEP,
      {
        once: 'process',
        text: 'Decodes your compiled program',
        tone: 'baml',
      },
      ...SHARED_LOAD,
    ],
  },
];

// What the build side makes for each target: an SDK for an app, one binary for standalone.
function buildFor(target: string, lang: number) {
  if (target === 'Standalone')
    return {
      cmd: 'baml pack',
      files: [
        {
          file: 'triage',
          note: 'the runtime and your program, one file',
          program: true,
        },
      ],
      out: 'dist/',
    };
  // The web SDK comes from the same TypeScript generator.
  const files = HOSTS[target === 'WebAssembly' ? 1 : lang].files;
  return { cmd: 'baml generate', files, out: 'baml_sdk/' };
}

function Generate({
  beat,
  build,
}: {
  beat: number;
  build: ReturnType<typeof buildFor>;
}) {
  // Beats 0-1 compile, 2 generates, then one file per beat, then a rest with everything shown.
  const files = Math.max(0, beat - 2);
  return (
    <div className="space-y-2">
      <div className="rounded-xl border border-white/15 bg-white/[0.02] px-4 py-3">
        <p className="font-mono text-[13px] text-head">baml_src/</p>
        <p className="mt-0.5 font-mono text-[12px] text-muted">
          triage.baml · invoices.baml
        </p>
      </div>
      <div
        className={`rounded-xl border px-4 py-3 font-mono text-[13px] transition-colors duration-300 ${beat >= 1 && beat <= 2 ? 'border-clover bg-clover/10 text-clover' : 'border-white/15 text-ink'}`}
      >
        <span className="text-muted">$ </span>
        {build.cmd}
        <span className="ml-3 text-[11px] text-muted">
          {beat < 1 ? '' : beat < 3 ? 'type-checking, compiling…' : '✓ done'}
        </span>
      </div>
      <div
        className="rounded-xl border border-clover/50 bg-clover/[0.05] p-3"
        data-sdk-src
      >
        <p className="font-mono text-[13px] text-clover">{build.out}</p>
        <div className="mt-2 space-y-1">
          {build.files.map((f, i) => (
            <p
              className={`flex items-baseline justify-between gap-3 rounded-md px-2 py-1 font-mono text-[12px] transition-[opacity,transform] duration-300 ${f.program ? 'bg-clover/15' : ''}`}
              key={f.file}
              style={{
                opacity: i < files ? 1 : 0,
                transform: `translateY(${i < files ? 0 : 4}px)`,
              }}
            >
              <span className={f.program ? 'text-clover' : 'text-ink'}>
                {f.file}
              </span>
              <span className="text-[11px] text-muted">{f.note}</span>
            </p>
          ))}
        </div>
      </div>
    </div>
  );
}

/** A box with a label in its top edge: something that contains other things. */
function Box({
  children,
  icon,
  label,
  sdk,
  tone = 'plain',
}: {
  children?: ReactNode;
  // A language's name in the label gets its logo here.
  icon?: string;
  label: string;
  // Marks the box the generated SDK lands in, so the hand-off animation can find it.
  sdk?: boolean;
  tone?: 'plain' | 'baml' | 'host';
}) {
  const border =
    tone === 'baml'
      ? 'border-clover/60 bg-clover/[0.06]'
      : tone === 'host'
        ? 'border-white/25 bg-white/[0.02]'
        : 'border-white/12 bg-white/[0.02]';
  return (
    <div
      className={`rounded-xl border p-3 transition-shadow duration-500 ${border}`}
      data-sdk={sdk ? '' : undefined}
    >
      <p
        className={`flex items-center gap-1.5 font-mono text-[12px] ${tone === 'baml' ? 'text-clover' : 'text-ink'}`}
      >
        {icon && <LangIcon className="size-3" name={icon} />}
        {label}
      </p>
      {children && <div className="mt-2.5 space-y-2">{children}</div>}
    </div>
  );
}

// Your threads, per language, as the host would name them.
const YOUR_THREADS: string[][] = [
  ['main thread', 'thread 2'],
  ['event loop'],
  ['goroutine', 'goroutine'],
  ['thread 1', 'thread 2'],
  ['thread 1', 'thread 2'],
  ['thread 1', 'thread 2'],
  ['task', 'task'],
];

function Threads({ names, tone }: { names: string[]; tone: 'baml' | 'host' }) {
  return (
    <div
      className="grid gap-1.5"
      style={{ gridTemplateColumns: `repeat(${names.length}, minmax(0, 1fr))` }}
    >
      {names.map((n, i) => (
        <span
          className={`rounded-md px-2 py-1 text-center font-mono text-[11px] ${tone === 'baml' ? 'bg-clover/20 text-head' : 'border border-white/20 text-ink'}`}
          // biome-ignore lint/suspicious/noArrayIndexKey: a fixed row of chips that never reorders.
          key={i}
        >
          {n}
        </span>
      ))}
    </div>
  );
}

// The runtime's thread pool: one worker per CPU core, every one named baml-worker. The last chip
// stands for the rest, so the picture never implies a fixed count.
const BAML_WORKERS = ['baml-worker', 'baml-worker', 'baml-worker', '…'];
const WORKERS_LABEL = 'BAML runtime, one worker thread per CPU core';

/** Where your side meets BAML's: a call goes across, its result comes back. */
function Bridge({
  label,
  note = 'A call crosses to BAML’s workers. Its result crosses back.',
}: {
  label: string;
  note?: string;
}) {
  return (
    <div className="relative flex items-center gap-3 rounded-xl border border-dashed border-clover/50 px-3 py-2.5">
      <span
        aria-hidden
        className="font-mono text-[15px] leading-none text-clover"
      >
        ⇅
      </span>
      <div className="min-w-0">
        <p className="font-mono text-[12px] text-clover">{label}</p>
        <p className="mt-0.5 text-[12px] text-muted">{note}</p>
      </div>
    </div>
  );
}

const TARGETS = [
  {
    draw: (lang: number) => (
      <Box icon={SDK_LANGS[lang].name} label={HOSTS[lang].process} tone="host">
        <div className="grid gap-2 sm:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)]">
          <Box label="your code, on your threads">
            <Threads names={YOUR_THREADS[lang]} tone="host" />
          </Box>
          <Box label="baml_sdk (generated)" sdk />
        </div>
        <Bridge label={HOSTS[lang].runtime} />
        <Box label={WORKERS_LABEL} tone="baml">
          <Threads names={BAML_WORKERS} tone="baml" />
        </Box>
      </Box>
    ),
    name: 'In your app',
    note: (lang: number) =>
      `BAML runs inside ${HOSTS[lang].process} and loads ${HOSTS[lang].loads}. Your code keeps its threads, BAML runs on its own, and the bridge connects them.`,
  },
  {
    draw: (_lang: number) => (
      <Box label="one binary, from baml pack" tone="host">
        <Box label={WORKERS_LABEL} tone="baml">
          <Threads names={BAML_WORKERS} tone="baml" />
          <Box label="your program, packed in" sdk tone="baml" />
        </Box>
      </Box>
    ),
    name: 'Standalone',
    note: () =>
      'No host language at all. baml pack builds one binary with your program inside, and each function becomes a subcommand.',
  },
  {
    draw: (_lang: number) => (
      <Box label="a browser tab, or a Cloudflare Worker" tone="host">
        <div className="grid gap-2 sm:grid-cols-[minmax(0,1.3fr)_minmax(0,1fr)]">
          <Box label="your JavaScript">
            <Threads names={['event loop']} tone="host" />
          </Box>
          <Box label="baml_sdk (generated)" sdk />
        </div>
        <Bridge
          label="the WebAssembly bridge"
          note="A call runs as a task on your event loop. Its result resolves a promise."
        />
        <Box label="BAML runtime · WebAssembly, on your event loop" tone="baml">
          <Threads names={['BAML tasks']} tone="baml" />
        </Box>
      </Box>
    ),
    name: 'WebAssembly',
    note: () =>
      'The same program in the browser or on Workers. WebAssembly is single-threaded here, so BAML runs its calls as tasks on your event loop, with no threads of its own.',
  },
];

// ---------- scenarios: what happens on startup, on a call, on an error, on shutdown ----------
// Sources: sdks/python/src/baml_bridge/__init__.py (atexit shutdown, unhandled spawn errors),
// sdks/python/rust/bridge_python/src/unhandled_spawn.rs (shutdown_runtime), crates/bridge_cffi
// (initialize_runtime_from_blob, catch_unwind), crates/baml_pack_host (packed binary).

/** A sequence, top to bottom: for startup and shutdown. */
function Steps({ beat: driven, steps }: { beat?: number; steps: Step[] }) {
  const own = useBeat(steps.length + 2, 1100);
  const beat = driven ?? own;
  return (
    <ol>
      {steps.map((s, i) => {
        const lit = i <= beat;
        const tone =
          s.tone === 'bad'
            ? 'border-[#f85149]/60 bg-[#f85149]/10'
            : s.tone === 'baml'
              ? 'border-clover/60 bg-clover/[0.08]'
              : 'border-white/15 bg-white/[0.02]';
        return (
          <li key={s.text}>
            {i > 0 && (
              <div aria-hidden className="py-1 pl-6 text-[12px] text-muted">
                ↓
              </div>
            )}
            <div
              className={`flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 rounded-xl border px-4 py-3 transition-opacity duration-300 ${tone}`}
              style={{
                boxShadow:
                  driven !== undefined && i === beat
                    ? '0 0 0 1px rgb(255 255 255 / 0.35)'
                    : undefined,
                opacity: lit ? 1 : 0.3,
              }}
            >
              <span className="text-[15px] text-head">{s.text}</span>
              {(s.once || s.note) && (
                <span className="flex flex-wrap items-center justify-end gap-2">
                  {s.note && (
                    <span className="font-mono text-[11px] text-muted [overflow-wrap:anywhere]">
                      {s.note}
                    </span>
                  )}
                  {s.once && (
                    <span className="whitespace-nowrap rounded-full border border-clover/50 px-2 py-0.5 font-mono text-[10.5px] text-clover">
                      once per {s.once}
                    </span>
                  )}
                </span>
              )}
            </div>
          </li>
        );
      })}
    </ol>
  );
}

const KIND: Record<Kind, { badge: string; box: string; label: string }> = {
  bug: {
    badge: 'bg-[#f85149] text-black',
    box: 'border-[#f85149]/60 bg-[#f85149]/[0.06]',
    label: 'Bug in BAML',
  },
  expected: {
    badge: 'bg-clover text-black',
    box: 'border-clover/40 bg-clover/[0.04]',
    label: 'Expected',
  },
  panic: {
    badge: 'bg-[#79c0ff] text-black',
    box: 'border-[#79c0ff]/40 bg-[#79c0ff]/[0.04]',
    label: 'Panic',
  },
  unhandled: {
    badge: 'bg-[#e3b341] text-black',
    box: 'border-[#e3b341]/50 bg-[#e3b341]/[0.05]',
    label: 'Unhandled',
  },
};

/** Where an exception ends up, sorted by kind: the ones you plan for, the ones nothing caught, and
 *  the ones that should never happen. */
function Exceptions({ cases }: { cases: Case[] }) {
  return (
    <div
      className={`grid gap-3 ${cases.length > 3 ? 'md:grid-cols-2 xl:grid-cols-4' : cases.length > 2 ? 'md:grid-cols-3' : 'md:grid-cols-2'}`}
    >
      {cases.map((c) => {
        const k = KIND[c.kind];
        return (
          <div
            className={`flex flex-col rounded-xl border p-4 ${k.box}`}
            key={c.when}
          >
            <span
              className={`self-start rounded-full px-2 py-0.5 font-mono text-[10px] uppercase tracking-wider ${k.badge}`}
            >
              {k.label}
            </span>
            <p className="mt-3 text-[14px] text-ink-2">{c.when}</p>
            <p className="mt-2 text-[15px] leading-snug text-head [overflow-wrap:anywhere]">
              {c.outcome}
            </p>
            {c.kind === 'panic' && (
              <div className="mt-4">
                <pre className="overflow-x-auto rounded-lg border border-white/10 bg-black/30 p-3 font-mono text-[12px] leading-relaxed text-ink">
                  {'let arr = [1, 2, 3];\n'}
                  <span className="text-[#79c0ff]">
                    {'// panics with IndexOutOfBounds\n'}
                  </span>
                  {'let x = arr[5];'}
                </pre>
                <p className="mt-2 text-[13px] leading-snug text-muted">
                  Also division by zero, a missing map key, a failed assert,
                  stack overflow, integer overflow, and baml.sys.panic(…). None
                  of these appear in a function’s throws.
                </p>
              </div>
            )}
            {c.kind === 'bug' && (
              <p className="mt-4 rounded-lg border border-[#f85149]/50 bg-black/30 p-3 text-[13px] leading-snug text-[#ff9b94]">
                ⚠ This should never happen. BAML should never crash or segfault
                your process. If you see one,{' '}
                <a
                  className="underline underline-offset-2 hover:text-head"
                  href="https://boundaryml.com/discord"
                  rel="noopener"
                  target="_blank"
                >
                  tell us on Discord
                </a>{' '}
                and we’ll fix it.
              </p>
            )}
          </div>
        );
      })}
    </div>
  );
}

// Where each sysop goes, in plain words: what BAML needs to do, and what it uses to do it.
// Checked in the code: native hosts run BAML's own Rust (crates/sys_native) on the shared process,
// so env vars, the working directory and exit are your process's; HTTP is BAML's own client
// (reqwest), which reads the proxy env vars; your functions are reached through call_host_value.
// The web build (crates/sys_wasm) takes fetch and readFileSync from JavaScript and has nothing else.
type Via = 'baml' | 'none' | 'process' | 'yours';
// `code` is the BAML that makes the call, so each row reads as a line you'd write.
type Route = {
  code: string;
  need: string;
  note?: string;
  uses: string;
  via: Via;
};

// How your own code changes the env and the working directory, per language; Java can't change
// either at run time.
const SETTERS: { cwd: string; env: string }[] = [
  { cwd: 'os.chdir', env: 'os.environ' },
  { cwd: 'process.chdir', env: 'process.env' },
  { cwd: 'os.Chdir', env: 'os.Setenv' },
  { cwd: 'std::env::set_current_dir', env: 'std::env::set_var' },
  { cwd: '', env: '' },
  { cwd: 'chdir', env: 'setenv' },
  { cwd: 'FileManager', env: 'setenv' },
];

function routesFor(target: string, lang: number): Route[] {
  const env = 'baml.env.get("OPENAI_API_KEY")';
  const read = 'baml.fs.read("prompts/system.txt")';
  const fetch = 'baml.http.fetch(url)';
  const callback = 'escalate(body)  // yours';
  const exit = 'baml.sys.exit(1)';
  if (target === 'WebAssembly')
    return [
      {
        code: fetch,
        need: 'Make an HTTP request',
        uses: 'your fetch()',
        via: 'yours',
      },
      {
        code: read,
        need: 'Read a file',
        uses: 'your readFileSync',
        via: 'yours',
      },
      {
        code: callback,
        need: 'Call your code',
        uses: 'your JavaScript functions',
        via: 'yours',
      },
      {
        code: 'baml.env.get(…), baml.sys.run(…), …',
        need: 'Anything else',
        uses: 'not available on the web',
        via: 'none',
      },
    ];
  if (target === 'Standalone')
    return [
      {
        code: env,
        need: 'Read an env var',
        uses: 'the environment it was launched with',
        via: 'process',
      },
      {
        code: read,
        need: 'Read a file',
        uses: 'the filesystem',
        via: 'process',
      },
      {
        code: fetch,
        need: 'Make an HTTP request',
        uses: 'BAML’s own HTTP client',
        via: 'baml',
      },
      { code: exit, need: 'Exit', uses: 'the binary exits', via: 'process' },
    ];
  const set = SETTERS[lang];
  const name = SDK_LANGS[lang].name;
  return [
    {
      code: env,
      need: 'Read an env var',
      note: set.env ? `the ones ${set.env} sets` : 'fixed when the JVM starts',
      uses: 'your process’s env vars',
      via: 'process',
    },
    {
      code: 'baml.sys.current_dir()',
      need: 'Get the working directory',
      note: set.cwd ? `${set.cwd} moves it` : 'fixed when the JVM starts',
      uses: 'your working directory',
      via: 'process',
    },
    {
      code: fetch,
      need: 'Make an HTTP request',
      note: 'it reads the proxy env vars',
      uses: 'BAML’s own HTTP client',
      via: 'baml',
    },
    {
      code: callback,
      need: 'Call your code',
      uses: `your ${name} functions`,
      via: 'yours',
    },
    {
      code: exit,
      need: 'Exit',
      uses: 'ends your whole process',
      via: 'process',
    },
  ];
}

// BAML's own work in the accent color, your code in white, the shared process in grey.
const VIA: Record<Via, string> = {
  baml: 'text-clover',
  none: 'text-[#ff9b94]',
  process: 'text-ink-2',
  yours: 'text-head',
};

/** What BAML uses when it needs the outside world, one row per need, lit in turn as the runtime
 *  sets its sysops up. */
function Bindings({
  active,
  lang,
  target,
  wired,
}: {
  active: boolean;
  lang: number;
  target: string;
  wired: number;
}) {
  const rows = routesFor(target, lang);
  return (
    <div
      className={`rounded-xl border p-4 transition-colors duration-300 md:p-5 ${active ? 'border-clover/70' : 'border-rule'} bg-raised`}
    >
      <p className="mb-4 text-[14px] leading-snug text-ink-2">
        A sysop is a function in BAML’s standard library that reaches outside
        BAML. Sysops tie BAML into{' '}
        {target === 'In your app'
          ? 'your language.'
          : target === 'Standalone'
            ? 'the operating system.'
            : 'JavaScript.'}
      </p>
      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)] gap-x-4 border-b border-rule pb-2 text-[12px] text-muted">
        <span>When BAML needs to</span>
        <span>it uses</span>
      </div>
      <ul>
        {rows.map((r, i) => {
          const on = i < wired;
          return (
            <li
              className="grid grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)] gap-x-4 border-b border-rule/60 py-2.5 transition-opacity duration-300 last:border-b-0"
              key={r.need}
              style={{ opacity: on ? 1 : 0.45 }}
            >
              <span className="min-w-0 text-[14px] leading-snug text-ink-2">
                {r.need}
                <code className="mt-1 block font-mono text-[11.5px] text-clover/90 [overflow-wrap:anywhere]">
                  {r.code}
                </code>
              </span>
              <span className="text-[14px] leading-snug">
                <span className={VIA[r.via]}>{r.uses}</span>
                {r.note && (
                  <span className="block text-[12.5px] text-muted">
                    {r.note}
                  </span>
                )}
              </span>
            </li>
          );
        })}
      </ul>
      {target !== 'WebAssembly' && envDemo(target, lang) && (
        <div className="mt-4 border-t border-rule pt-4">
          <p className="text-[13px] text-ink-2">
            For example, change a variable between two calls:
          </p>
          <pre className="mt-2 overflow-x-auto rounded-lg border border-rule bg-black/20 px-3 py-2.5 font-mono text-[11.5px] leading-[1.6] text-body">
            {envDemo(target, lang)
              ?.split('\n')
              .map((line, i) => (
                // biome-ignore lint/suspicious/noArrayIndexKey: the sample's lines never reorder.
                <DemoLine key={i} line={line} />
              ))}
          </pre>
        </div>
      )}
    </div>
  );
}

/** Startup's steps in two bands: what happens once per machine (the cached download, which can
 *  also happen ahead of time) and what happens once per process. The step that triggers it sits
 *  above them, and a last line says what every later call skips. */
function StartupSteps({ beat, steps }: { beat: number; steps: Step[] }) {
  const lit = (i: number) => ({ opacity: i <= beat ? 1 : 0.3 });
  const row = (st: Step, i: number) => (
    <div
      className={`rounded-lg border px-3.5 py-2.5 transition-opacity duration-300 ${st.tone === 'host' ? 'border-white/15 bg-white/[0.02]' : 'border-clover/40 bg-clover/[0.06]'}`}
      key={st.text}
      style={lit(i)}
    >
      <p className="text-[15px] text-head">{st.text}</p>
      {st.note && (
        <p className="mt-0.5 font-mono text-[11.5px] text-muted">{st.note}</p>
      )}
    </div>
  );
  const groups: { once?: Step['once']; items: [Step, number][] }[] = [];
  steps.forEach((st, i) => {
    const last = groups.at(-1);
    if (last && last.once === st.once) last.items.push([st, i]);
    else groups.push({ items: [[st, i]], once: st.once });
  });
  return (
    <div className="space-y-3">
      {groups.map((g) =>
        g.once ? (
          <div
            className="rounded-xl border border-dashed border-clover/40 p-3"
            key={g.items[0][0].text}
          >
            <p className="mb-2 font-mono text-[11px] uppercase tracking-wider text-clover">
              Once per {g.once}
            </p>
            <div className="space-y-2">
              {g.items.map(([st, i]) => row(st, i))}
            </div>
            {g.once === 'machine' && (
              <p className="mt-2 text-[12.5px] leading-snug text-muted">
                Or ahead of time, in your build.{' '}
                <a
                  className="text-ink-2 underline underline-offset-2 hover:text-head"
                  href="#release"
                >
                  See release
                </a>
                .
              </p>
            )}
          </div>
        ) : (
          <div key={g.items[0][0].text}>
            {g.items.map(([st, i]) => row(st, i))}
          </div>
        ),
      )}
      <p
        className="px-1 text-[14px] text-ink-2 transition-opacity duration-300"
        style={lit(steps.length)}
      >
        Every call after this goes straight to the runtime.
      </p>
    </div>
  );
}

/** Startup's steps and the sysop wiring on one clock, so the wires draw while their step runs. */
function Startup({
  lang = 0,
  steps,
  target,
}: {
  lang?: number;
  steps: Step[];
  target: string;
}) {
  const at = steps.findIndex((x) => x.sysops);
  const wires = routesFor(target, lang).length;
  const tick = useBeat(steps.length + wires + 4, 900);
  const step = tick <= at ? tick : tick <= at + wires ? at : tick - wires;
  return (
    <div className="grid grid-cols-[minmax(0,1fr)] items-start gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
      <StartupSteps beat={step} steps={steps} />
      <Bindings
        active={step === at}
        lang={lang}
        target={target}
        wired={tick < at ? 0 : tick - at}
      />
    </div>
  );
}

const SCENARIOS = ['Startup', 'A call', 'An exception', 'Shutdown'] as const;
type Scenario = (typeof SCENARIOS)[number];

/** What each target does in each scenario. A call reuses the nested picture of where things live. */
const BEHAVIOR: Record<string, Record<Scenario, ReactNode>> = {
  Standalone: {
    'A call': null,
    'An exception': (
      <Exceptions
        cases={[
          {
            kind: 'expected',
            outcome: 'It’s handled right there, like in any language.',
            when: 'Your BAML code catches it',
          },
          {
            kind: 'panic',
            outcome:
              'Catch it in BAML like any other exception. If nothing does, the binary exits with a failure code.',
            when: 'Your BAML code panics, like dividing by zero or a failed assert',
          },
          {
            kind: 'unhandled',
            outcome:
              'BAML prints it, and the binary exits with a failure code.',
            when: 'Nothing catches it',
          },
          {
            kind: 'bug',
            outcome:
              'BAML reports an SdkPanic instead of crashing. A crash during garbage collection still aborts.',
            when: 'The BAML runtime itself fails',
          },
        ]}
      />
    ),
    Shutdown: (
      <Steps
        steps={[
          { text: 'The function you ran returns', tone: 'baml' },
          { text: 'Its result is printed', tone: 'baml' },
          { text: 'The binary exits', tone: 'host' },
        ]}
      />
    ),
    Startup: (
      <Startup
        steps={[
          { text: './triage classify-ticket "…"', tone: 'host' },
          {
            sysops: true,
            text: 'Sets up its sysops',
            tone: 'baml',
          },
          {
            note: 'packed into the binary',
            text: 'Reads your program from inside itself',
            tone: 'baml',
          },
          { text: 'Starts BAML’s worker threads', tone: 'baml' },
          {
            note: 'args parsed from its signature',
            text: 'Runs the function you named',
            tone: 'baml',
          },
        ]}
        target="Standalone"
      />
    ),
  },
  WebAssembly: {
    'A call': null,
    'An exception': (
      <Exceptions
        cases={[
          {
            kind: 'expected',
            outcome: 'The Promise rejects. Catch it with try/catch.',
            when: 'Your BAML function throws an error it declares',
          },
          {
            kind: 'panic',
            outcome: 'The Promise rejects. Catch it with try/catch.',
            when: 'Your BAML code panics, like dividing by zero or a failed assert',
          },
          {
            kind: 'bug',
            outcome: 'WebAssembly can’t recover from it, so the module stops.',
            when: 'The BAML runtime itself fails',
          },
        ]}
      />
    ),
    Shutdown: (
      <Steps
        steps={[
          { text: 'The tab closes, or the Worker finishes', tone: 'host' },
          { text: 'The WebAssembly module goes with it', tone: 'baml' },
        ]}
      />
    ),
    Startup: (
      <Startup
        steps={[
          { text: 'import from baml_sdk', tone: 'host' },
          { text: 'Loads the WebAssembly runtime', tone: 'baml' },
          {
            sysops: true,
            text: 'Takes fetch and readFileSync from JavaScript',
            tone: 'baml',
          },
          { text: 'Decodes your compiled program', tone: 'baml' },
        ]}
        target="WebAssembly"
      />
    ),
  },
};

function behaviorFor(
  target: string,
  lang: number,
): Record<Scenario, ReactNode> {
  if (target !== 'In your app') return BEHAVIOR[target];
  const h = HOSTS[lang];
  return {
    'A call': null,
    'An exception': <Exceptions cases={h.exceptions} />,
    Shutdown: <Steps steps={h.shutdown} />,
    Startup: <Startup lang={lang} steps={h.startup} target="In your app" />,
  };
}

/** The finished SDK, carried from the build side into the box it lands in on the run side. */
function useHandOff(beat: number, fly: number) {
  const wrap = useRef<HTMLDivElement>(null);
  const [flight, setFlight] = useState<{
    at: 'from' | 'to';
    from: { x: number; y: number };
    to: { x: number; y: number };
  } | null>(null);
  useEffect(() => {
    const root = wrap.current;
    if (!root) return;
    if (beat !== fly) {
      if (beat < fly) setFlight(null);
      return;
    }
    const src = root.querySelector('[data-sdk-src]');
    const dst = root.querySelector('[data-sdk]');
    if (!src || !dst) return;
    const r = root.getBoundingClientRect();
    const a = src.getBoundingClientRect();
    const b = dst.getBoundingClientRect();
    const from = { x: a.left - r.left + 12, y: a.top - r.top + 10 };
    const to = { x: b.left - r.left + b.width - 120, y: b.top - r.top + 6 };
    setFlight({ at: 'from', from, to });
    const frame = requestAnimationFrame(() =>
      requestAnimationFrame(() =>
        setFlight((f) => (f ? { ...f, at: 'to' } : f)),
      ),
    );
    return () => cancelAnimationFrame(frame);
  }, [beat, fly]);
  return { flight, wrap };
}

function Architecture({ at, lang }: { at: number; lang: number }) {
  const target = TARGETS[at];
  const build = buildFor(target.name, lang);
  // Two beats to compile, one to generate, one per file, then the hand-off and a rest.
  const fly = build.files.length + 3;
  const beat = useBeat(build.files.length + 6, 700);
  const { flight, wrap } = useHandOff(beat, fly);
  const landed = beat > fly;
  return (
    <div
      className="relative grid items-start gap-8 lg:grid-cols-[minmax(0,22rem)_auto_minmax(0,1fr)]"
      ref={wrap}
    >
      {flight && (
        <span
          aria-hidden
          className="pointer-events-none absolute left-0 top-0 z-10 hidden rounded-md lg:block border border-clover bg-clover px-2.5 py-1 font-mono text-[12px] text-black shadow-[0_0_24px_rgb(from_var(--color-clover)_r_g_b/0.5)] transition-transform duration-[900ms] ease-in-out"
          style={{
            opacity: landed ? 0 : 1,
            transform: `translate(${(flight.at === 'from' ? flight.from : flight.to).x}px, ${(flight.at === 'from' ? flight.from : flight.to).y}px)`,
          }}
        >
          {build.out}
        </span>
      )}
      <div>
        <p className="font-display text-[20px] text-head">When you build</p>
        <p className="mt-1 text-[14px] text-muted">
          On your machine, or in CI.
        </p>
        <div className="mt-5">
          <Generate beat={beat} build={build} />
        </div>
      </div>
      <div className="flex items-center justify-center gap-2 self-stretch text-muted lg:flex-col">
        <span
          aria-hidden
          className="text-[22px] leading-none text-clover lg:rotate-0 max-lg:rotate-90"
        >
          →
        </span>
        <p className="text-center text-[13px] leading-snug">
          ships with
          <br className="max-lg:hidden" /> your app
        </p>
      </div>
      <div>
        <div className="flex flex-wrap items-baseline justify-between gap-3">
          <p className="font-display text-[20px] text-head">When it runs</p>
        </div>
        <p className="mt-1 min-h-[3em] text-[14px] leading-snug text-muted">
          {target.note(lang)}
        </p>
        <div
          className={`mt-4 [&_[data-sdk]]:transition-shadow ${landed ? '[&_[data-sdk]]:shadow-[0_0_0_2px_var(--color-clover)]' : ''}`}
        >
          {target.draw(lang)}
        </div>
      </div>
    </div>
  );
}

function TargetPicker({
  at,
  setAt,
}: {
  at: number;
  setAt: (i: number) => void;
}) {
  return (
    <div className="inline-flex rounded-full border border-white/15 p-1">
      {TARGETS.map((t, i) => (
        <button
          aria-pressed={i === at}
          className={`focus-ring cursor-pointer rounded-full px-3 py-1 text-[13px] transition-colors ${i === at ? 'bg-clover text-black' : 'text-ink-2 hover:text-head'}`}
          key={t.name}
          onClick={() => setAt(i)}
          type="button"
        >
          {t.name}
        </button>
      ))}
    </div>
  );
}

// What ships to production, per target and language, and what to set so it runs the same there.
// Sources: release/platforms.json, ENV_VARS.md (Bridges), the Go and Rust loaders
// (sdks/go/baml_go/runtime_artifact.go, sdks/rust/bridge_rust/src/loader/mod.rs, which has
// ensure_library_cached for warming the cache), crates/baml_cli/src/pack_command.rs.
type Release = { ships: string; steps: { code?: string; text: string }[] };

const RELEASE_APP: Release[] = [
  {
    ships:
      'The runtime is inside the baml_bridge wheel, for Linux (glibc and Alpine), macOS and Windows, on x64 and arm64.',
    steps: [
      {
        code: 'RUN pip install -r requirements.txt',
        text: 'Install your requirements in the image. There’s nothing else to fetch.',
      },
    ],
  },
  {
    ships:
      'The runtime is a prebuilt Node addon for all eight platforms, Alpine included.',
    steps: [
      {
        code: 'RUN npm ci',
        text: 'Install inside the image, so npm picks the addon for its platform.',
      },
    ],
  },
  {
    ships:
      'The runtime is a shared library. By default it downloads once per machine and version, on the first call, into $BAML_HOME/bridges, checked against its SHA-256.',
    steps: [
      { code: 'ENV CGO_ENABLED=1', text: 'Build with cgo.' },
      {
        code: 'COPY libbridge_cffi.so /opt/baml/\nENV BAML_BRIDGE_PATH=/opt/baml/libbridge_cffi.so\nENV BAML_BRIDGE_DISABLE_DOWNLOAD=1',
        text: 'Ship the library in the image and point BAML at it, so production never downloads.',
      },
    ],
  },
  {
    ships:
      'The runtime is a shared library. By default it downloads once per machine and version, on the first call, into $BAML_HOME/bridges, checked against its SHA-256.',
    steps: [
      {
        code: 'baml_bridge::loader::ensure_library_cached()?;',
        text: 'Warm the cache in your build stage.',
      },
      {
        code: 'COPY --from=build /root/.baml/bridges /opt/baml/bridges\nENV BAML_HOME=/opt/baml\nENV BAML_BRIDGE_DISABLE_DOWNLOAD=1',
        text: 'Copy the cache into the image and turn downloads off.',
      },
    ],
  },
  {
    ships:
      'The runtime is a JNI library inside the jar, unpacked when it loads. Alpine and Windows on arm64 are experimental.',
    steps: [
      { text: 'Build your jar as usual. There’s nothing else to fetch.' },
    ],
  },
  {
    ships: 'The runtime is a shared library, and it’s never downloaded.',
    steps: [
      {
        code: 'COPY libbridge_cffi.so /app/',
        text: 'Put it next to your executable, or point BAML_BRIDGE_PATH at it.',
      },
    ],
  },
  {
    ships:
      'The runtime is a static library, linked into your app when you build.',
    steps: [{ text: 'Nothing else ships.' }],
  },
];

const RELEASE_OTHER: Record<string, Release> = {
  Standalone: {
    ships: 'One executable with the runtime and your program inside.',
    steps: [
      {
        code: 'baml pack -f classify_ticket --target x86_64-unknown-linux-gnu',
        text: 'Build for the machine it runs on. --target takes any of the eight platforms.',
      },
    ],
  },
  WebAssembly: {
    ships: 'The runtime is WebAssembly, bundled with your JavaScript.',
    steps: [{ text: 'Bundle and deploy as usual. Nothing native to install.' }],
  },
};

/** What goes into the image for the picked target, then what holds for every target. */
function ReleaseView({ at, lang }: { at: number; lang: number }) {
  const name = TARGETS[at].name;
  const r = name === 'In your app' ? RELEASE_APP[lang] : RELEASE_OTHER[name];
  return (
    <div className="grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,22rem)]">
      <div>
        <p className="text-[16px] leading-snug text-head">{r.ships}</p>
        <ol className="mt-5 space-y-4">
          {r.steps.map((step, i) => (
            <li
              className="grid grid-cols-[1.75rem_minmax(0,1fr)] gap-3"
              key={step.text}
            >
              <span className="grid size-6 place-items-center rounded-full border border-white/20 font-mono text-[12px] text-ink-2">
                {i + 1}
              </span>
              <div className="min-w-0">
                <p className="text-[15px] leading-snug text-ink-2">
                  {step.text}
                </p>
                {step.code && (
                  <pre className="mt-2 overflow-x-auto rounded-lg border border-rule bg-raised px-3.5 py-2.5 font-mono text-[13px] leading-[1.6] text-ink">
                    {step.code}
                  </pre>
                )}
              </div>
            </li>
          ))}
        </ol>
      </div>
      <div className="space-y-3">
        <div className="rounded-xl border border-rule bg-raised p-4">
          <p className="font-mono text-[11px] uppercase tracking-wider text-clover">
            Pin one version
          </p>
          <p className="mt-2 text-[14px] leading-snug text-ink-2">
            Keep baml.toml’s toolchain version, your SDK package and the
            generated code on the same version. If they drift, startup fails
            with a version skew error.
          </p>
        </div>
        <div className="rounded-xl border border-rule bg-raised p-4">
          <p className="font-mono text-[11px] uppercase tracking-wider text-clover">
            In production
          </p>
          <p className="mt-2 text-[14px] leading-snug text-ink-2">
            BOUNDARY_API_KEY sends recordings to Boundary Cloud. BAML_TELEMETRY
            sets how much is recorded: off, low, medium (the default) or high.
          </p>
        </div>
      </div>
    </div>
  );
}

// What happens when your code changes an environment variable, the clearest way to see sysops at
// work. Sources: baml_std/baml/ns_env/env.baml (env.X is a Ref holding the name, read at use time),
// baml_std/ai/spec.baml (api_key = env.OPENAI_API_KEY is read per request), sys_native io_impls.rs
// (env.get is std::env::var_os; the HTTP client is cached per HTTP_CLIENT_ENV values, so a proxy or
// CA change applies to the next request), sys_wasm (no env on the web).
// The same two calls in each language, with the key changed in between. null where the language
// can't change its own environment.
const ENV_DEMO: (string | null)[] = [
  'os.environ["OPENAI_API_KEY"] = "sk-old"\nawait classify_ticket(body)  # sends sk-old\n\nos.environ["OPENAI_API_KEY"] = "sk-new"\nawait classify_ticket(body)  # sends sk-new',
  'process.env.OPENAI_API_KEY = "sk-old";\nawait classify_ticket(body); // sends sk-old\n\nprocess.env.OPENAI_API_KEY = "sk-new";\nawait classify_ticket(body); // sends sk-new',
  'os.Setenv("OPENAI_API_KEY", "sk-old")\nbaml_sdk.ClassifyTicket(ctx, body) // sends sk-old\n\nos.Setenv("OPENAI_API_KEY", "sk-new")\nbaml_sdk.ClassifyTicket(ctx, body) // sends sk-new',
  'unsafe { std::env::set_var("OPENAI_API_KEY", "sk-old") }\nclassify_ticket(body).await?; // sends sk-old\n\nunsafe { std::env::set_var("OPENAI_API_KEY", "sk-new") }\nclassify_ticket(body).await?; // sends sk-new',
  null,
  'setenv("OPENAI_API_KEY", "sk-old", 1);\nbaml_sdk::classify_ticket(body); // sends sk-old\n\nsetenv("OPENAI_API_KEY", "sk-new", 1);\nbaml_sdk::classify_ticket(body); // sends sk-new',
  'setenv("OPENAI_API_KEY", "sk-old", 1)\ntry await classify_ticket(body: body) // sends sk-old\n\nsetenv("OPENAI_API_KEY", "sk-new", 1)\ntry await classify_ticket(body: body) // sends sk-new',
];

const STANDALONE_DEMO =
  '$ OPENAI_API_KEY=sk-old ./triage classify-ticket "…"  # sends sk-old\n$ OPENAI_API_KEY=sk-new ./triage classify-ticket "…"  # sends sk-new';

/** The before-and-after sample for a target and language, or null where there's nothing to change
 *  (Java, and the web). */
function envDemo(target: string, lang: number): string | null {
  if (target === 'WebAssembly') return null;
  return target === 'Standalone' ? STANDALONE_DEMO : ENV_DEMO[lang];
}

/** One sample line, with its trailing comment in the accent color: the comment is the point. */
function DemoLine({ line }: { line: string }) {
  const at = line.search(/ (#|\/\/) /);
  if (at < 0) return <span className="block min-h-[1lh]">{line}</span>;
  return (
    <span className="block">
      {line.slice(0, at)}
      <span className="text-clover">{line.slice(at)}</span>
    </span>
  );
}

/** Change a variable between two calls and the second call sees it: BAML reads the process
 *  environment at the request, it doesn't keep a copy. */
function EnvChange({ at, lang }: { at: number; lang: number }) {
  const name = TARGETS[at].name;
  if (name === 'WebAssembly')
    return (
      <p className="max-w-[44rem] text-[15px] leading-snug text-ink-2">
        There’s no process environment in a browser or a Worker, so env.get
        isn’t available there. Calling it panics with HostUnavailable.
      </p>
    );
  const code = envDemo(name, lang);
  return (
    <div className="grid items-start gap-8 lg:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)]">
      {code ? (
        <div className="overflow-hidden rounded-xl border border-rule bg-raised">
          <p className="flex h-10 items-center gap-2 border-b border-rule px-3.5 font-mono text-[12px] text-ink-2">
            {name === 'In your app' && (
              <LangIcon className="size-3.5" name={SDK_LANGS[lang].name} />
            )}
            {name === 'In your app' ? 'your code' : 'your shell'}
          </p>
          <pre className="overflow-x-auto px-3.5 py-3 font-mono text-[12.5px] leading-[1.65] text-body">
            {code.split('\n').map((line, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: the sample's lines never reorder.
              <DemoLine key={i} line={line} />
            ))}
          </pre>
        </div>
      ) : (
        <p className="text-[15px] leading-snug text-ink-2">
          Java can’t change its own environment, so BAML reads what the JVM
          started with, the same as System.getenv.
        </p>
      )}
      <ul className="space-y-4 text-[15px] leading-snug text-ink-2">
        <li>
          <span className="text-head">It reads at the request.</span>{' '}
          env.OPENAI_API_KEY holds the variable’s name, not its value, so a
          secret you load after startup is used from the next call on.
        </li>
        <li>
          <span className="text-head">BAML only reads.</span> There’s no
          env.set: changes come from your code. For a single command,
          baml.sys.run takes env overrides that only that child process sees.
        </li>
        <li>
          <span className="text-head">Clients never hold the key.</span>{' '}
          Printing or recording a client can’t leak it.
        </li>
        <li>
          <span className="text-head">Proxies follow too.</span> Change
          HTTPS_PROXY, NO_PROXY or SSL_CERT_FILE and the next request uses them.
        </li>
        <li className="text-[13px] text-muted">
          Set variables before starting calls on other threads. Changing the
          environment isn’t thread-safe in C, or in any language built on it.
        </li>
      </ul>
    </div>
  );
}

const MOMENTS: {
  lead: string;
  name: Exclude<Scenario, 'A call'>;
  title: string;
}[] = [
  {
    lead: 'Only the first call pays for this. Along the way the runtime sets up its sysops: the way BAML reaches the outside world, like env vars, files and the network.',
    name: 'Startup',
    title: 'What happens at startup',
  },
  {
    lead: 'What if your BAML code has an error? It depends on the kind.',
    name: 'An exception',
    title: 'What happens when BAML throws an exception',
  },
  {
    lead: 'BAML finishes its work before your program exits.',
    name: 'Shutdown',
    title: 'What happens at shutdown',
  },
];

/** The page's two choices: what BAML runs in, and for an app, its language. Pinned to the top so
 *  every diagram below follows it. */
/** How far down a sticky bar has to sit to clear the site's own sticky nav, if the page has one,
 *  kept current as the nav's height changes (it's taller on some widths). */
function useNavHeight() {
  const [height, setHeight] = useState(0);
  useEffect(() => {
    const nav = [...document.querySelectorAll('header')].find(
      (el) => getComputedStyle(el).position === 'sticky',
    );
    if (!nav) return;
    const measure = () => setHeight(nav.getBoundingClientRect().height);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(nav);
    return () => observer.disconnect();
  }, []);
  return height;
}

function ControlBar({
  at,
  lang,
  setAt,
  setLang,
}: {
  at: number;
  lang: number;
  setAt: (i: number) => void;
  setLang: (i: number) => void;
}) {
  const top = useNavHeight();
  return (
    <div
      className="sticky z-20 -mx-(--frame-pad) border-y border-rule bg-ground/90 px-(--frame-pad) py-3 backdrop-blur"
      style={{ top }}
    >
      <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
        <TargetPicker at={at} setAt={setAt} />
        {TARGETS[at].name === 'In your app' ? (
          <LangPicker at={lang} setAt={setLang} />
        ) : (
          <p className="text-[13px] text-muted">
            {TARGETS[at].name === 'Standalone'
              ? 'No host language: one binary.'
              : 'In the browser or on Cloudflare Workers.'}
          </p>
        )}
      </div>
    </div>
  );
}

function Section({
  children,
  id,
  lead,
  title,
}: {
  children: ReactNode;
  id?: string;
  lead: string;
  title: string;
}) {
  return (
    <section className="scroll-mt-40 border-t border-rule pt-12 pb-16" id={id}>
      <h2 className="font-display text-[28px] leading-tight text-head md:text-[32px]">
        {title}
      </h2>
      <p className="mt-2 max-w-[40rem] text-[16px] text-ink-2">{lead}</p>
      <div className="mt-8">{children}</div>
    </section>
  );
}

/** The page as it would ship: the architecture first, then one diagram per moment in a program's
 *  life, all for the environment picked in the bar. */
export function ArchitecturePage({ embedded }: { embedded?: boolean }) {
  const [at, setAt] = useState(0);
  const [lang, setLang] = usePinnedLang();
  const target = TARGETS[at];
  return (
    <>
      <header className="pb-10">
        {embedded ? (
          <h2 className="v4-h2 text-head">How BAML runs</h2>
        ) : (
          <h1 className="v4-h2 text-head">How BAML runs</h1>
        )}
        <p className="mt-4 max-w-[42rem] text-[17px] leading-relaxed text-ink-2">
          {embedded
            ? 'BAML compiles when you build and runs inside your app as a library, with no sidecar and no server. Pick where it runs, and the diagrams below follow.'
            : 'BAML compiles when you build and runs inside your app as a library. There’s no sidecar and no server. Pick where it runs, and every diagram on this page follows.'}
        </p>
      </header>
      <ControlBar at={at} lang={lang} setAt={setAt} setLang={setLang} />
      <Section
        lead="baml generate type-checks and compiles your BAML, then writes an SDK in your language with the compiled program inside. Your app ships with it."
        title="Build, then run"
      >
        <Architecture at={at} lang={lang} />
      </Section>
      <Section
        id="release"
        lead="What ships with your app, and what to set so production runs the same as your machine."
        title="What happens at release"
      >
        <div key={`${target.name}-${lang}`}>
          <ReleaseView at={at} lang={lang} />
        </div>
      </Section>
      {MOMENTS.map((m) => (
        <Section key={m.name} lead={m.lead} title={m.title}>
          <div
            className={m.name === 'Shutdown' ? 'max-w-[52rem]' : ''}
            key={`${target.name}-${lang}`}
          >
            {behaviorFor(target.name, lang)[m.name]}
          </div>
        </Section>
      ))}
      <Section
        lead="The simplest way to see sysops at work: BAML reads the environment when it needs a value, instead of keeping its own copy."
        title="What happens when you change an environment variable"
      >
        <div key={`${target.name}-${lang}`}>
          <EnvChange at={at} lang={lang} />
        </div>
      </Section>
    </>
  );
}
