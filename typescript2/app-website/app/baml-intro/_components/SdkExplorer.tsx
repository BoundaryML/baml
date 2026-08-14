'use client';

import { useState } from 'react';
import type { IconType } from 'react-icons';
import { SiGo, SiPython, SiRust, SiTypescript } from 'react-icons/si';
import { BamlCode } from '../../learn2/_components/BamlCode';

/*
 * One BAML file on the left; on the right, the generated SDK + usage for the
 * chosen host language. Two layers of toggles: pick a *feature* (functions,
 * classes/methods, generics) and a *language* (Python, TypeScript, Go, Rust).
 *
 * Python and TypeScript track the real `baml generate` output (pydantic v2
 * models / typed classes, sync+async pairs, `$generic` type args). Go and Rust
 * are in progress, so their panes are illustrative and badged.
 *
 * Ground truth: baml_language/sdk_tests/crates/{python_pydantic2,typescript_node}.
 */

type SdkLang = 'python' | 'typescript' | 'go' | 'rust';
// Languages with a shipped SDK; Go/Rust show a "join the Discord" placeholder.
type ReadyLang = 'python' | 'typescript';
const COMING_SOON: ReadonlySet<SdkLang> = new Set(['go', 'rust']);
const DISCORD_URL = 'https://boundaryml.com/discord';

interface Pane {
  filename: string;
  lang: ReadyLang;
  code: string;
  note: string;
}

interface Feature {
  id: 'functions' | 'classes' | 'generics';
  label: string;
  baml: string;
  bamlFile: string;
  panes: Record<ReadyLang, Pane>;
}

const LANGS: { id: SdkLang; label: string; Icon: IconType; color: string }[] = [
  { color: '#3776AB', Icon: SiPython, id: 'python', label: 'Python' },
  { color: '#3178C6', Icon: SiTypescript, id: 'typescript', label: 'TypeScript' },
  { color: '#00ADD8', Icon: SiGo, id: 'go', label: 'Go' },
  { color: '#1a1612', Icon: SiRust, id: 'rust', label: 'Rust' },
];

const FEATURES: Feature[] = [
  // ── functions ──────────────────────────────────────────────────────────
  {
    id: 'functions',
    label: 'Functions',
    bamlFile: 'resume.baml',
    baml: `class Resume {
  name: string,
  email: string?,
}

function extract_resume(text: string) -> Resume {
  client: "openai/gpt-4o-mini"
  prompt: \`Extract the resume. \${ctx.output_format}\\n\${text}\`
}`,
    panes: {
      python: {
        filename: 'baml_sdk',
        lang: 'python',
        note: 'Every function generates a sync + async pair; Resume is a real pydantic v2 model.',
        code: `from baml_sdk import extract_resume, Resume

# typed call — or: await extract_resume_async(text=...)
resume: Resume = extract_resume(text="Jane Doe, jane@acme.com ...")

resume.name          # str
resume.email         # str | None
resume.model_dump()  # plain pydantic underneath`,
      },
      typescript: {
        filename: 'baml_sdk',
        lang: 'typescript',
        note: 'Resume is a typed class; extract_resume_async returns a Promise.',
        code: `import { extract_resume, type Resume } from './baml_sdk';

// typed call — or: await extract_resume_async('...')
const resume: Resume = extract_resume('Jane Doe, jane@acme.com ...');

resume.name;   // string
resume.email;  // string | null`,
      },
    },
  },
  // ── classes / methods ──────────────────────────────────────────────────
  {
    id: 'classes',
    label: 'Classes / methods',
    bamlFile: 'greeter.baml',
    baml: `class Greeter {
  name: string,

  // instance method
  function greet(self, greeting: string) -> string {
    \`\${greeting}, \${self.name}\`
  }

  // static factory
  function create(name: string) -> Greeter {
    Greeter { name: name }
  }
}`,
    panes: {
      python: {
        filename: 'baml_sdk',
        lang: 'python',
        note: 'BAML methods come through bound — static factory on the class, instance method on the value.',
        code: `from baml_sdk import Greeter

g = Greeter.create("Ada")   # static factory -> Greeter
g.greet("hello")            # "hello, Ada" — the BAML method, bound
g.name                      # "Ada"

await g.greet_async("hi")   # every callable has an _async twin`,
      },
      typescript: {
        filename: 'baml_sdk',
        lang: 'typescript',
        note: 'Static methods sit on the class; instance methods are bound to the value.',
        code: `import { Greeter } from './baml_sdk';

const g = Greeter.create('Ada');  // static factory -> Greeter
g.greet('hello');                 // "hello, Ada" — the BAML method, bound
g.name;                           // "Ada"

await g.greet_async('hi');        // async twin`,
      },
    },
  },
  // ── generics ───────────────────────────────────────────────────────────
  {
    id: 'generics',
    label: 'Generics',
    bamlFile: 'box.baml',
    baml: `class Box<T> {
  value: T,

  function get(self) -> T { self.value }
}

function identity<T>(x: T) -> T { x }`,
    panes: {
      python: {
        filename: 'baml_sdk',
        lang: 'python',
        note: 'Type params flow through: subscript syntax fn[T](...) carries the BAML generic.',
        code: `from baml_sdk import Box, identity

# generic args via subscript: fn[T](...)
identity[int](5)          # 5  (typed int)
identity[str]("hi")       # "hi"

box = Box[int](value=42)  # Box[int]
box.get()                 # 42 (typed T)`,
      },
      typescript: {
        filename: 'baml_sdk',
        lang: 'typescript',
        note: 'BAML <T> maps straight onto a TypeScript generic; Box<number> stays typed end to end.',
        code: `import { Box, identity } from './baml_sdk';

// generic args in angle brackets
identity<number>(5);      // 5  (typed number)
identity<string>('hi');   // "hi"

const box = new Box<number>({ value: 42 });  // Box<number>
box.get();                // 42 (typed T)`,
      },
    },
  },
];

/**
 * Two-layer SDK explorer: choose a language feature and a host language to see
 * one BAML file generate the corresponding typed SDK + usage.
 */
export function SdkExplorer() {
  const [featureId, setFeatureId] = useState<Feature['id']>('functions');
  const [lang, setLang] = useState<SdkLang>('python');
  const feature = FEATURES.find((f) => f.id === featureId) ?? FEATURES[0];
  const comingSoon = COMING_SOON.has(lang);
  const pane = comingSoon ? null : feature.panes[lang as ReadyLang];
  const langLabel = LANGS.find((l) => l.id === lang)?.label ?? lang;

  return (
    <div>
      <div aria-label="Feature" className="l6-sdk-tabs" role="tablist">
        {FEATURES.map((f) => (
          <button
            aria-selected={featureId === f.id}
            className={`l6-sdk-tab font-mono${featureId === f.id ? ' l6-sdk-tab--on' : ''}`}
            key={f.id}
            onClick={() => setFeatureId(f.id)}
            role="tab"
            type="button"
          >
            {f.label}
          </button>
        ))}
      </div>
      <div aria-label="SDK language" className="l6-sdk-tabs" role="tablist">
        {LANGS.map(({ id, label, Icon, color }) => (
          <button
            aria-selected={lang === id}
            className={`l6-sdk-tab font-mono${lang === id ? ' l6-sdk-tab--on' : ''}`}
            key={id}
            onClick={() => setLang(id)}
            role="tab"
            type="button"
          >
            <Icon aria-hidden color={color} size={14} />
            {label}
            {(id === 'go' || id === 'rust') && (
              <span className="l6-sdk-soon">in progress</span>
            )}
          </button>
        ))}
      </div>
      <div className="l6-pair">
        <div>
          <p className="l6-pane-label">the baml file</p>
          <BamlCode code={feature.baml} filename={feature.bamlFile} />
        </div>
        <div>
          <p className="l6-pane-label l6-pane-label--after">
            generated sdk · {lang}
          </p>
          {comingSoon || !pane ? (
            <div className="l6-sdk-soon-pane">
              <p>
                {`Join our `}
                <a
                  className="l6-link"
                  href={DISCORD_URL}
                  rel="noreferrer"
                  target="_blank"
                >
                  Discord
                </a>
                {` if you're interested in the ${langLabel} SDK.`}
              </p>
            </div>
          ) : (
            <>
              <BamlCode
                code={pane.code}
                filename={pane.filename}
                lang={pane.lang}
              />
              <p className="l6-dim" style={{ marginTop: '0.6rem' }}>
                {pane.note}
              </p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
