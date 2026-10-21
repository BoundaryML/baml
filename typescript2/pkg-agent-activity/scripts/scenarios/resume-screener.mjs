// A scripted session that exercises every kind of event the UI handles:
//
//   - several small files created in a row in one directory (baml_src/)
//   - files created in other directories (fixtures/, tests/)
//   - a burst of generated files written within a second (src/baml_client/)
//   - large files written in one go (the generated types and client)
//   - a sequence of edits to one file (src/index.ts)
//   - deletions (the legacy regex parser and its test)
//   - runs: `baml run` with inputs, outputs, and the functions that executed;
//     a failing test run, the fix, and the passing rerun; plain commands
//   - a commit
//
// The project is a small TypeScript CLI that screens resumes, moving from a
// regex parser to BAML extraction. Timing follows measured agent rhythm:
// short bursts a few seconds apart, then a minute or so of thinking.

import { ScenarioBuilder } from '../scenario-builder.mjs';

const RESUME = 'baml_src/resume.baml';
const JOB = 'baml_src/job.baml';
const SCORING = 'baml_src/scoring.baml';
const CLIENTS = 'baml_src/clients.baml';
const INDEX = 'src/index.ts';

export function resumeScreener() {
  const b = new ScenarioBuilder({
    branch: 'agent/baml-extraction',
    description:
      'Scaffold BAML files, add fixtures, generate a client, rewire the CLI, delete the regex parser, fail a test, fix the prompt, and rank candidates.',
    id: 'resume-screener',
    project: 'resume-screener',
    startHead: { sha: '4e1c07a2d', subject: 'Add parse command' },
    title: 'Build a resume screener',
  });

  b.baseline('package.json', PACKAGE_JSON);
  b.baseline('README.md', README);
  b.baseline(INDEX, INDEX_TS);
  b.baseline('src/legacy/regex_parser.ts', REGEX_PARSER);
  b.baseline('src/legacy/patterns.ts', PATTERNS);
  b.baseline('tests/parser.test.ts', PARSER_TEST);

  // ── Scaffold: small files in a row, one directory ─────────────────────────
  b.at(1500).note(
    "I'll move resume parsing from the regex parser to BAML. First the BAML project: a client, then the resume and job types.",
  );
  b.at(4200).write('baml.toml', BAML_TOML);
  b.wait(5100).write(CLIENTS, CLIENTS_BAML);
  b.wait(6400).write(RESUME, RESUME_BAML_V1);
  b.wait(4700).write(JOB, JOB_BAML);

  b.wait(7800);
  let run = b.start({
    command: 'baml check',
    kind: 'command',
    touches: [CLIENTS, RESUME, JOB],
  });
  b.wait(1300).finish(run, {
    log: 'Checked 3 files in baml_src: no errors.',
    status: 'passed',
  });

  // ── Fixtures: small files in a row, another directory ─────────────────────
  b.wait(38500).note(
    '`baml check` passes. Adding three sample resumes with different layouts, so extraction is tested on more than one format.',
  );
  b.wait(2500).write('fixtures/resumes/ada_lovelace.txt', ADA);
  b.wait(3100).write('fixtures/resumes/grace_hopper.txt', GRACE);
  b.wait(2600).write('fixtures/resumes/alan_turing.txt', ALAN);

  b.wait(7800).note(
    "Running ExtractResume on Ada's resume to see what the model returns.",
  );
  b.wait(1400);
  run = b.start({
    command:
      'baml run ExtractResume --args-file fixtures/resumes/ada_lovelace.txt',
    input: { text: '(fixtures/resumes/ada_lovelace.txt, 1,124 chars)' },
    kind: 'baml-run',
    target: {
      line: lineOf(RESUME_BAML_V1, 'function ExtractResume'),
      name: 'ExtractResume',
      path: RESUME,
    },
    touches: [RESUME, CLIENTS, 'fixtures/resumes/ada_lovelace.txt'],
  });
  b.wait(3400).finish(run, {
    frames: [
      {
        durationMs: 3312,
        line: lineOf(RESUME_BAML_V1, 'function ExtractResume'),
        llm: {
          client: 'Fast',
          inputTokens: 612,
          model: 'gpt-5-mini',
          outputTokens: 188,
        },
        name: 'ExtractResume',
        path: RESUME,
      },
    ],
    output: ADA_OUTPUT,
    status: 'passed',
  });

  // ── Generate: a burst of files within a second, two of them large ─────────
  b.wait(49000).note(
    'The output has every field, with experience newest first. Generating the TypeScript client.',
  );
  b.wait(3000);
  run = b.start({
    command: 'baml generate',
    kind: 'command',
    touches: [RESUME, JOB, CLIENTS, 'baml.toml'],
  });
  b.wait(900).write('src/baml_client/index.ts', CLIENT_INDEX);
  b.wait(180).write('src/baml_client/types.ts', CLIENT_TYPES);
  b.wait(210).write('src/baml_client/async_client.ts', ASYNC_CLIENT);
  b.wait(160).write('src/baml_client/sync_client.ts', SYNC_CLIENT);
  b.wait(190).write('src/baml_client/type_builder.ts', TYPE_BUILDER);
  b.wait(300).finish(run, {
    log: 'Generated 5 files in src/baml_client (typescript/node).',
    status: 'passed',
  });
  b.wait(6200).edit(
    'package.json',
    '  "dependencies": {\n',
    '  "dependencies": {\n    "@boundaryml/baml": "^0.15.1",\n',
  );

  // ── Rewire the CLI: a sequence of edits to one file ───────────────────────
  b.wait(41000).note(
    'Now rewiring `src/index.ts` to call `b.ExtractResume` instead of the regex parser, and adding a `--compact` flag.',
  );
  b.wait(3000).edit(
    INDEX,
    "import { parseResume } from './legacy/regex_parser.js';",
    "import { b } from './baml_client/index.js';\nimport type { Resume } from './baml_client/types.js';",
  );
  b.wait(5200).edit(
    INDEX,
    '    const resume = parseResume(text);\n',
    '    const resume: Resume = await b.ExtractResume(text);\n',
  );
  b.wait(6800).edit(
    INDEX,
    "  .action(async (file: string) => {\n    const text = await readFile(file, 'utf8');",
    "  .option('--compact', 'print one line per resume')\n  .action(async (file: string, opts: { compact?: boolean }) => {\n    const text = await readFile(file, 'utf8');",
  );
  b.wait(4400).edit(
    INDEX,
    '    console.log(JSON.stringify(resume, null, 2));\n',
    '    console.log(opts.compact ? summarize(resume) : JSON.stringify(resume, null, 2));\n',
  );
  b.wait(5900).edit(
    INDEX,
    'program.parseAsync();\n',
    'function summarize(resume: Resume): string {\n  const years = resume.experience.length;\n  return `${resume.name}: ${resume.headline} (${years} roles, ${resume.skills.length} skills)`;\n}\n\nprogram.parseAsync();\n',
  );

  // ── Delete the legacy parser ──────────────────────────────────────────────
  b.wait(29000).note(
    'Nothing imports the legacy parser anymore, so I am deleting it.',
  );
  b.wait(2000).delete('src/legacy/regex_parser.ts');
  b.wait(2200).delete('src/legacy/patterns.ts');

  // ── Tests: replace the old test, run, fail ────────────────────────────────
  b.wait(35500).note(
    'Replacing the parser test with tests that run ExtractResume on the three fixtures.',
  );
  b.wait(2500).delete('tests/parser.test.ts');
  b.wait(4100).write('tests/extract.test.ts', EXTRACT_TEST);
  b.wait(8800);
  run = b.start({
    command: 'pnpm vitest run tests/extract.test.ts',
    kind: 'command',
    touches: [
      'tests/extract.test.ts',
      RESUME,
      'src/baml_client/async_client.ts',
    ],
  });
  b.wait(11800).finish(run, {
    log: [
      ' ✓ tests/extract.test.ts > ExtractResume > reads the name and email (3.1s)',
      ' × tests/extract.test.ts > ExtractResume > lists skills for every candidate (4.4s)',
      '   → expected [] to have a length of at least 3 but got +0',
      '     grace_hopper.txt: skills section is titled "Specialties"',
      ' ✓ tests/extract.test.ts > ExtractResume > orders experience newest first (3.8s)',
      '',
      ' Test Files  1 failed (1)',
      '      Tests  1 failed | 2 passed (3)',
    ].join('\n'),
    status: 'failed',
  });

  // ── Fix the prompt, rerun ─────────────────────────────────────────────────
  b.wait(9000).note(
    'Grace\'s resume lists skills under "Specialties", so `skills` came back empty. I\'ll describe the field and tell the prompt to read any heading.',
  );
  b.wait(27000).edit(
    RESUME,
    '    skills: string[],\n',
    '    skills: string[] @description("Every skill, tool, or specialty the candidate lists, under any heading"),\n',
  );
  b.wait(6100).edit(
    RESUME,
    "        Extract the candidate's resume.\n",
    '        Extract the candidate\'s resume. Sections may use unusual headings\n        ("Specialties", "Toolbox", "What I know"); read them all.\n',
  );
  b.wait(8000).note("Checking Grace's resume again.");
  b.wait(1400);
  run = b.start({
    command:
      'baml run ExtractResume --args-file fixtures/resumes/grace_hopper.txt',
    input: { text: '(fixtures/resumes/grace_hopper.txt, 986 chars)' },
    kind: 'baml-run',
    target: {
      line: lineOf(RESUME_BAML_V1, 'function ExtractResume'),
      name: 'ExtractResume',
      path: RESUME,
    },
    touches: [RESUME, CLIENTS, 'fixtures/resumes/grace_hopper.txt'],
  });
  b.wait(2900).finish(run, {
    frames: [
      {
        durationMs: 2861,
        line: lineOf(RESUME_BAML_V1, 'function ExtractResume'),
        llm: {
          client: 'Fast',
          inputTokens: 655,
          model: 'gpt-5-mini',
          outputTokens: 201,
        },
        name: 'ExtractResume',
        path: RESUME,
      },
    ],
    output: GRACE_OUTPUT,
    status: 'passed',
  });
  b.wait(7200);
  run = b.start({
    command: 'pnpm vitest run',
    kind: 'command',
    touches: ['tests/extract.test.ts', RESUME],
  });
  b.wait(12600).finish(run, {
    log: [
      ' ✓ tests/extract.test.ts (3 tests) 11.2s',
      '',
      ' Test Files  1 passed (1)',
      '      Tests  3 passed (3)',
    ].join('\n'),
    status: 'passed',
  });

  // ── Ranking: a new file, edits across three files, a multi-function run ──
  b.wait(2500).note('All three tests pass.');
  b.wait(58000).note(
    'Next, ranking: ScoreCandidate on the careful client, and RankForJob to extract each resume and score it against the posting.',
  );
  b.wait(3500).write(SCORING, SCORING_BAML);
  b.wait(5600).write('fixtures/jobs/staff_engineer.md', JOB_POSTING);
  b.wait(7300).edit(
    JOB,
    '    remote: bool,\n',
    '    remote: bool,\n    must_have: string[] @description("Requirements the posting marks as required"),\n',
  );
  b.wait(12000).edit(
    INDEX,
    'function summarize(',
    "program\n  .command('rank')\n  .argument('<job>', 'job posting as markdown')\n  .argument('<resumes...>', 'resumes as plain text')\n  .action(async (job: string, files: string[]) => {\n    const posting = await readFile(job, 'utf8');\n    const texts = await Promise.all(files.map((f) => readFile(f, 'utf8')));\n    const ranked = await b.RankForJob(posting, texts);\n    for (const r of ranked) console.log(`${r.score.toFixed(2)}  ${r.name}  ${r.reason}`);\n  });\n\nfunction summarize(",
  );
  b.wait(8600).note(
    'Ranking the three candidates against the staff engineer posting.',
  );
  b.wait(1800);
  const rankLine = lineOf(SCORING_BAML, 'function RankForJob');
  const scoreLine = lineOf(SCORING_BAML, 'function ScoreCandidate');
  const jobLine = lineOf(JOB_BAML, 'function ParseJobPosting');
  const extractLine = lineOf(RESUME_BAML_V1, 'function ExtractResume');
  run = b.start({
    command: 'baml run RankForJob --args-file fixtures/rank_args.json',
    input: {
      job_text: '(fixtures/jobs/staff_engineer.md)',
      resume_texts: ['ada_lovelace.txt', 'grace_hopper.txt', 'alan_turing.txt'],
    },
    kind: 'baml-run',
    target: { line: rankLine, name: 'RankForJob', path: SCORING },
    touches: [SCORING, JOB, RESUME, CLIENTS],
  });
  b.wait(9800).finish(run, {
    frames: [
      { durationMs: 9712, line: rankLine, name: 'RankForJob', path: SCORING },
      {
        durationMs: 1840,
        line: jobLine,
        llm: {
          client: 'Fast',
          inputTokens: 540,
          model: 'gpt-5-mini',
          outputTokens: 122,
        },
        name: 'ParseJobPosting',
        path: JOB,
      },
      {
        durationMs: 2790,
        line: extractLine,
        llm: {
          client: 'Fast',
          inputTokens: 612,
          model: 'gpt-5-mini',
          outputTokens: 190,
        },
        name: 'ExtractResume',
        path: RESUME,
      },
      {
        durationMs: 2655,
        line: extractLine,
        llm: {
          client: 'Fast',
          inputTokens: 655,
          model: 'gpt-5-mini',
          outputTokens: 204,
        },
        name: 'ExtractResume',
        path: RESUME,
      },
      {
        durationMs: 2402,
        line: extractLine,
        llm: {
          client: 'Fast',
          inputTokens: 590,
          model: 'gpt-5-mini',
          outputTokens: 176,
        },
        name: 'ExtractResume',
        path: RESUME,
      },
      {
        durationMs: 1410,
        line: scoreLine,
        llm: {
          client: 'Careful',
          inputTokens: 1180,
          model: 'claude-sonnet-5',
          outputTokens: 96,
        },
        name: 'ScoreCandidate',
        path: SCORING,
      },
      {
        durationMs: 1377,
        line: scoreLine,
        llm: {
          client: 'Careful',
          inputTokens: 1204,
          model: 'claude-sonnet-5',
          outputTokens: 88,
        },
        name: 'ScoreCandidate',
        path: SCORING,
      },
      {
        durationMs: 1296,
        line: scoreLine,
        llm: {
          client: 'Careful',
          inputTokens: 1131,
          model: 'claude-sonnet-5',
          outputTokens: 91,
        },
        name: 'ScoreCandidate',
        path: SCORING,
      },
    ],
    output: RANK_OUTPUT,
    status: 'passed',
  });

  // ── Wrap up: docs, commit ─────────────────────────────────────────────────
  b.wait(6000).note(
    "Grace ranks first, which matches the posting's must-haves. Updating the README, then committing.",
  );
  b.wait(41000).edit(
    'README.md',
    'The parser is regex based (see `src/legacy/`), so it misses anything that\ndoes not follow the usual section headings.\n',
    'Resumes and postings are read by BAML functions in `baml_src/`; the\ngenerated client lives in `src/baml_client/` (run `baml generate` after\nediting `.baml` files).\n\n    pnpm screen rank fixtures/jobs/staff_engineer.md fixtures/resumes/*.txt\n',
  );
  b.wait(4800).write('.env.example', 'OPENAI_API_KEY=\nANTHROPIC_API_KEY=\n');
  b.wait(14000).commit(
    '9b2f4e1c7',
    'Replace the regex resume parser with BAML extraction',
  );
  return b.build();
}

function lineOf(text, needle) {
  return text.split('\n').findIndex((l) => l.includes(needle));
}

// ── Baseline files ──────────────────────────────────────────────────────────

const PACKAGE_JSON = `{
  "name": "resume-screener",
  "version": "0.3.0",
  "private": true,
  "type": "module",
  "scripts": {
    "screen": "tsx src/index.ts",
    "test": "vitest run"
  },
  "dependencies": {
    "commander": "^12.1.0"
  },
  "devDependencies": {
    "tsx": "^4.19.2",
    "typescript": "^5.6.3",
    "vitest": "^2.1.4"
  }
}
`;

const README = `# resume-screener

Screens resumes against a job posting.

## Usage

    pnpm screen parse path/to/resume.txt

The parser is regex based (see \`src/legacy/\`), so it misses anything that
does not follow the usual section headings.
`;

const INDEX_TS = `import { readFile } from 'node:fs/promises';
import { Command } from 'commander';
import { parseResume } from './legacy/regex_parser.js';

const program = new Command();

program
  .name('resume-screener')
  .description('Screen resumes against a job posting');

program
  .command('parse')
  .argument('<file>', 'resume as plain text')
  .action(async (file: string) => {
    const text = await readFile(file, 'utf8');
    const resume = parseResume(text);
    console.log(JSON.stringify(resume, null, 2));
  });

program.parseAsync();
`;

const REGEX_PARSER = `import { EMAIL, HEADINGS, YEAR_RANGE } from './patterns.js';

export interface ParsedResume {
  name: string;
  email: string | null;
  skills: string[];
  experience: { company: string; years: string }[];
}

/** Splits a resume into sections by the headings in HEADINGS. */
function sections(text: string): Map<string, string[]> {
  const out = new Map<string, string[]>();
  let current = 'header';
  for (const line of text.split('\\n')) {
    const heading = HEADINGS.find((h) => h.test(line));
    if (heading) {
      current = heading.source;
      continue;
    }
    const list = out.get(current) ?? [];
    list.push(line.trim());
    out.set(current, list);
  }
  return out;
}

export function parseResume(text: string): ParsedResume {
  const parts = sections(text);
  const header = parts.get('header') ?? [];
  const skills = (parts.get(HEADINGS[1].source) ?? [])
    .flatMap((l) => l.split(','))
    .map((s) => s.trim())
    .filter(Boolean);
  const experience = (parts.get(HEADINGS[0].source) ?? [])
    .map((l) => ({ line: l, years: YEAR_RANGE.exec(l)?.[0] }))
    .filter((e) => e.years)
    .map((e) => ({ company: e.line.replace(YEAR_RANGE, '').trim(), years: e.years! }));
  return {
    name: header[0] ?? '',
    email: EMAIL.exec(text)?.[0] ?? null,
    skills,
    experience,
  };
}
`;

const PATTERNS = `export const EMAIL = /[\\w.+-]+@[\\w-]+\\.[\\w.]+/;

export const YEAR_RANGE = /(19|20)\\d{2}\\s*[-–]\\s*((19|20)\\d{2}|present)/i;

/** Section headings, in the order they usually appear. */
export const HEADINGS = [
  /^\\s*(work )?experience\\s*$/i,
  /^\\s*skills\\s*$/i,
  /^\\s*education\\s*$/i,
];
`;

const PARSER_TEST = `import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { parseResume } from '../src/legacy/regex_parser.js';

const sample = readFileSync('tests/data/sample.txt', 'utf8');

describe('parseResume', () => {
  it('reads the name from the first line', () => {
    expect(parseResume(sample).name).toBe('Jordan Example');
  });

  it('finds the email anywhere', () => {
    expect(parseResume(sample).email).toBe('jordan@example.com');
  });

  it('splits skills on commas', () => {
    expect(parseResume(sample).skills).toContain('TypeScript');
  });
});
`;

// ── BAML ────────────────────────────────────────────────────────────────────

const BAML_TOML = `[package]
name = "resume_screener"

[generator.typescript]
output_type = "typescript/node"
output_dir = "src"
`;

const CLIENTS_BAML = `client<llm> Fast {
    provider: openai,
    options: {
        model: "gpt-5-mini",
    },
}

client<llm> Careful {
    provider: anthropic,
    options: {
        model: "claude-sonnet-5",
        max_tokens: 1024,
    },
}
`;

const RESUME_BAML_V1 = `class Experience {
    company: string,
    title: string,
    start_year: int,
    end_year: int? @description("null while the role is current"),
}

class Resume {
    name: string,
    email: string?,
    headline: string @description("Current role and seniority, in one line"),
    experience: Experience[] @description("Newest first"),
    skills: string[],
}

function ExtractResume(text: string) -> Resume {
    client: Fast
    prompt: \`
        \${role("system")}
        Extract the candidate's resume.
        \${role("user")}
        \${text}
    \`
}

test "extracts a minimal resume" {
    let r = ExtractResume("Jordan Example\\njordan@example.com\\nStaff engineer at Initech, 2019-present")
    assert.equal(r.name, "Jordan Example")
}
`;

const JOB_BAML = `class JobPosting {
    title: string,
    level: string @description("junior, mid, senior, staff, or principal"),
    remote: bool,
}

function ParseJobPosting(text: string) -> JobPosting {
    client: Fast
    prompt: \`
        \${role("system")}
        Read this job posting.
        \${role("user")}
        \${text}
    \`
}
`;

const SCORING_BAML = `class Score {
    score: float @description("0 to 1: how well the candidate fits the posting"),
    reason: string @description("One sentence, citing the resume"),
    missing: string[] @description("Required qualifications the resume does not show"),
}

class Ranked {
    name: string,
    score: float,
    reason: string,
}

function ScoreCandidate(resume: Resume, job: JobPosting) -> Score {
    client: Careful
    prompt: \`
        \${role("system")}
        You screen candidates for the role below. Be strict about must-haves.
        \${role("user")}
        Role: \${job.title} (\${job.level})
        Must have: \${job.must_have}
        Candidate: \${resume.name}, \${resume.headline}
        Skills: \${resume.skills}
    \`
}

function RankForJob(job_text: string, resume_texts: string[]) -> Ranked[] {
    let job = ParseJobPosting(job_text)
    let ranked: Ranked[] = []
    for (text in resume_texts) {
        let resume = ExtractResume(text)
        let s = ScoreCandidate(resume, job)
        ranked.push(Ranked { name: resume.name, score: s.score, reason: s.reason })
    }
    ranked.sort_by(|r| -r.score)
}
`;

// ── Fixtures ────────────────────────────────────────────────────────────────

const ADA = `Ada Lovelace
ada@analytical.engine

Principal Engineer, Computing Machinery

Experience
Analytical Engine Co., Principal Engineer, 1842 - present
  Wrote the first published algorithm intended for a machine.
  Translated and annotated Menabrea's memoir; the notes run three times longer.
Babbage & Associates, Research Collaborator, 1833 - 1842
  Worked with the Difference Engine team on tables and error checking.

Skills
Algorithms, mathematical analysis, technical writing, Bernoulli numbers

Education
Private tutoring in mathematics and science
`;

const GRACE = `GRACE HOPPER
grace.hopper@navy.mil | Arlington, VA

Rear Admiral (retired); compiler pioneer

Where I have worked
- Digital Equipment Corporation: Senior Consultant (1986 - 1992)
- US Navy: Director, Navy Programming Languages Group (1967 - 1986)
- Remington Rand: Senior Mathematician, UNIVAC (1949 - 1967)

Specialties
compilers (A-0, FLOW-MATIC), COBOL standardization, debugging,
teaching, standards committees, machine-independent languages

Education
PhD, Mathematics, Yale University
`;

const ALAN = `Alan Turing — alan@bletchley.gov.uk

Experience
2019–present  University of Manchester, Deputy Director, Computing Machine Laboratory
2012–2019     National Physical Laboratory, Designer, Automatic Computing Engine
2008–2012     Government Code and Cypher School, Head of Hut 8

Toolbox: cryptanalysis · automata theory · morphogenesis · long-distance running

Education: King's College, Cambridge (Mathematics); Princeton (PhD)
`;

const JOB_POSTING = `# Staff Engineer, Language Tooling

We are hiring a staff engineer to lead our compiler and developer tools.

## You will
- Own the compiler pipeline from parser to code generation
- Design language features with the product team
- Mentor engineers across two teams

## Must have
- Shipped a compiler or interpreter used by others
- Experience standardizing a language or file format
- Clear technical writing

## Nice to have
- Formal methods or type theory
- Remote-first experience
`;

const ADA_OUTPUT = {
  email: 'ada@analytical.engine',
  experience: [
    {
      company: 'Analytical Engine Co.',
      end_year: null,
      start_year: 1842,
      title: 'Principal Engineer',
    },
    {
      company: 'Babbage & Associates',
      end_year: 1842,
      start_year: 1833,
      title: 'Research Collaborator',
    },
  ],
  headline: 'Principal Engineer, computing machinery',
  name: 'Ada Lovelace',
  skills: [
    'Algorithms',
    'Mathematical analysis',
    'Technical writing',
    'Bernoulli numbers',
  ],
};

const GRACE_OUTPUT = {
  email: 'grace.hopper@navy.mil',
  experience: [
    {
      company: 'Digital Equipment Corporation',
      end_year: 1992,
      start_year: 1986,
      title: 'Senior Consultant',
    },
    {
      company: 'US Navy',
      end_year: 1986,
      start_year: 1967,
      title: 'Director, Navy Programming Languages Group',
    },
    {
      company: 'Remington Rand',
      end_year: 1967,
      start_year: 1949,
      title: 'Senior Mathematician, UNIVAC',
    },
  ],
  headline: 'Rear Admiral (retired), compiler pioneer',
  name: 'Grace Hopper',
  skills: [
    'Compilers (A-0, FLOW-MATIC)',
    'COBOL standardization',
    'Debugging',
    'Teaching',
    'Standards committees',
    'Machine-independent languages',
  ],
};

const RANK_OUTPUT = [
  {
    name: 'Grace Hopper',
    reason:
      'Built A-0 and FLOW-MATIC and led COBOL standardization; every must-have is shown.',
    score: 0.94,
  },
  {
    name: 'Ada Lovelace',
    reason:
      'Exceptional technical writing and algorithm design; no shipped compiler.',
    score: 0.71,
  },
  {
    name: 'Alan Turing',
    reason:
      'Designed the ACE and deep theory; no language standardization work listed.',
    score: 0.66,
  },
];

// ── Generated client (abridged but shaped like the real output) ─────────────

const CLIENT_INDEX = `// This file was generated by BAML: do not edit it. Run \`baml generate\` instead.

export { b } from './async_client.js';
export { b as bSync } from './sync_client.js';
export type * from './types.js';
export { TypeBuilder } from './type_builder.js';
`;

const CLIENT_TYPES = `// This file was generated by BAML: do not edit it. Run \`baml generate\` instead.

/** Generated from class Experience in baml_src/resume.baml */
export interface Experience {
  company: string;
  title: string;
  start_year: number;
  /** null while the role is current */
  end_year: number | null;
}

/** Generated from class Resume in baml_src/resume.baml */
export interface Resume {
  name: string;
  email: string | null;
  /** Current role and seniority, in one line */
  headline: string;
  /** Newest first */
  experience: Experience[];
  skills: string[];
}

/** Generated from class JobPosting in baml_src/job.baml */
export interface JobPosting {
  title: string;
  /** junior, mid, senior, staff, or principal */
  level: string;
  remote: boolean;
}

export namespace partial_types {
  export interface Experience {
    company?: string | null;
    title?: string | null;
    start_year?: number | null;
    end_year?: number | null;
  }

  export interface Resume {
    name?: string | null;
    email?: string | null;
    headline?: string | null;
    experience: partial_types.Experience[];
    skills: string[];
  }

  export interface JobPosting {
    title?: string | null;
    level?: string | null;
    remote?: boolean | null;
  }
}

export type StreamState<T> = {
  value: T;
  state: 'pending' | 'incomplete' | 'complete';
};

export interface Checked<T, CheckName extends string = string> {
  value: T;
  checks: Record<CheckName, { name: CheckName; expression: string; status: 'succeeded' | 'failed' }>;
}

export function all_succeeded<CheckName extends string>(
  checks: Record<CheckName, { status: 'succeeded' | 'failed' }>,
): boolean {
  return Object.values(checks).every((c) => (c as { status: string }).status === 'succeeded');
}

export function get_checks<CheckName extends string>(
  checks: Record<CheckName, { name: CheckName; status: 'succeeded' | 'failed' }>,
): { name: CheckName; status: 'succeeded' | 'failed' }[] {
  return Object.values(checks);
}
`;

const ASYNC_CLIENT = `// This file was generated by BAML: do not edit it. Run \`baml generate\` instead.

import type { BamlRuntime, BamlCtxManager, ClientRegistry, Collector } from '@boundaryml/baml';
import { toBamlError, type HTTPRequest } from '@boundaryml/baml';
import type { JobPosting, Resume, partial_types } from './types.js';
import { DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_CTX, DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME } from './globals.js';
import type { TypeBuilder } from './type_builder.js';

export type BamlCallOptions = {
  tb?: TypeBuilder;
  clientRegistry?: ClientRegistry;
  collector?: Collector | Collector[];
  env?: Record<string, string | undefined>;
  signal?: AbortSignal;
};

export class BamlAsyncClient {
  private runtime: BamlRuntime;
  private ctxManager: BamlCtxManager;
  private streamClient: BamlStreamClient;
  private httpRequest: AsyncHttpRequest;

  constructor(runtime: BamlRuntime, ctxManager: BamlCtxManager, private bamlOptions: BamlCallOptions = {}) {
    this.runtime = runtime;
    this.ctxManager = ctxManager;
    this.streamClient = new BamlStreamClient(runtime, ctxManager, bamlOptions);
    this.httpRequest = new AsyncHttpRequest(runtime, ctxManager);
  }

  withOptions(bamlOptions: BamlCallOptions): BamlAsyncClient {
    return new BamlAsyncClient(this.runtime, this.ctxManager, bamlOptions);
  }

  get stream(): BamlStreamClient {
    return this.streamClient;
  }

  get request(): AsyncHttpRequest {
    return this.httpRequest;
  }

  async ExtractResume(text: string, options?: BamlCallOptions): Promise<Resume> {
    try {
      const opts = { ...this.bamlOptions, ...(options ?? {}) };
      const raw = await this.runtime.callFunction(
        'ExtractResume',
        { text },
        this.ctxManager.cloneContext(),
        opts.tb?.__tb(),
        opts.clientRegistry,
        opts.collector ? (Array.isArray(opts.collector) ? opts.collector : [opts.collector]) : [],
        { ...process.env, ...(opts.env ?? {}) },
        opts.signal,
      );
      return raw.parsed(false) as Resume;
    } catch (error) {
      throw toBamlError(error);
    }
  }

  async ParseJobPosting(text: string, options?: BamlCallOptions): Promise<JobPosting> {
    try {
      const opts = { ...this.bamlOptions, ...(options ?? {}) };
      const raw = await this.runtime.callFunction(
        'ParseJobPosting',
        { text },
        this.ctxManager.cloneContext(),
        opts.tb?.__tb(),
        opts.clientRegistry,
        opts.collector ? (Array.isArray(opts.collector) ? opts.collector : [opts.collector]) : [],
        { ...process.env, ...(opts.env ?? {}) },
        opts.signal,
      );
      return raw.parsed(false) as JobPosting;
    } catch (error) {
      throw toBamlError(error);
    }
  }
}

class BamlStreamClient {
  constructor(
    private runtime: BamlRuntime,
    private ctxManager: BamlCtxManager,
    private bamlOptions: BamlCallOptions,
  ) {}

  ExtractResume(text: string, options?: BamlCallOptions) {
    const opts = { ...this.bamlOptions, ...(options ?? {}) };
    return this.runtime.streamFunction<partial_types.Resume, Resume>('ExtractResume', { text }, this.ctxManager.cloneContext(), opts);
  }

  ParseJobPosting(text: string, options?: BamlCallOptions) {
    const opts = { ...this.bamlOptions, ...(options ?? {}) };
    return this.runtime.streamFunction<partial_types.JobPosting, JobPosting>('ParseJobPosting', { text }, this.ctxManager.cloneContext(), opts);
  }
}

class AsyncHttpRequest {
  constructor(
    private runtime: BamlRuntime,
    private ctxManager: BamlCtxManager,
  ) {}

  async ExtractResume(text: string): Promise<HTTPRequest> {
    return this.runtime.buildRequest('ExtractResume', { text }, this.ctxManager.cloneContext(), false);
  }

  async ParseJobPosting(text: string): Promise<HTTPRequest> {
    return this.runtime.buildRequest('ParseJobPosting', { text }, this.ctxManager.cloneContext(), false);
  }
}

export const b = new BamlAsyncClient(
  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME,
  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_CTX,
);
`;

const SYNC_CLIENT = `// This file was generated by BAML: do not edit it. Run \`baml generate\` instead.

import type { BamlRuntime, BamlCtxManager } from '@boundaryml/baml';
import { toBamlError } from '@boundaryml/baml';
import type { JobPosting, Resume } from './types.js';
import type { BamlCallOptions } from './async_client.js';
import { DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_CTX, DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME } from './globals.js';

export class BamlSyncClient {
  constructor(
    private runtime: BamlRuntime,
    private ctxManager: BamlCtxManager,
    private bamlOptions: BamlCallOptions = {},
  ) {}

  ExtractResume(text: string, options?: BamlCallOptions): Resume {
    try {
      const opts = { ...this.bamlOptions, ...(options ?? {}) };
      return this.runtime.callFunctionSync('ExtractResume', { text }, this.ctxManager.cloneContext(), opts).parsed(false) as Resume;
    } catch (error) {
      throw toBamlError(error);
    }
  }

  ParseJobPosting(text: string, options?: BamlCallOptions): JobPosting {
    try {
      const opts = { ...this.bamlOptions, ...(options ?? {}) };
      return this.runtime.callFunctionSync('ParseJobPosting', { text }, this.ctxManager.cloneContext(), opts).parsed(false) as JobPosting;
    } catch (error) {
      throw toBamlError(error);
    }
  }
}

export const b = new BamlSyncClient(
  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME,
  DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_CTX,
);
`;

const TYPE_BUILDER = `// This file was generated by BAML: do not edit it. Run \`baml generate\` instead.

import { TypeBuilder as _TypeBuilder, type ClassBuilder } from '@boundaryml/baml/type_builder';

export class TypeBuilder {
  private tb: _TypeBuilder;
  Experience: ClassBuilder<'Experience', 'company' | 'title' | 'start_year' | 'end_year'>;
  Resume: ClassBuilder<'Resume', 'name' | 'email' | 'headline' | 'experience' | 'skills'>;
  JobPosting: ClassBuilder<'JobPosting', 'title' | 'level' | 'remote'>;

  constructor() {
    this.tb = new _TypeBuilder({
      classes: new Set(['Experience', 'Resume', 'JobPosting']),
      enums: new Set([]),
    });
    this.Experience = this.tb.classBuilder('Experience', ['company', 'title', 'start_year', 'end_year']);
    this.Resume = this.tb.classBuilder('Resume', ['name', 'email', 'headline', 'experience', 'skills']);
    this.JobPosting = this.tb.classBuilder('JobPosting', ['title', 'level', 'remote']);
  }

  __tb() {
    return this.tb._tb();
  }
}
`;

const EXTRACT_TEST = `import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { b } from '../src/baml_client/index.js';

const fixture = (name: string) => readFileSync(\`fixtures/resumes/\${name}.txt\`, 'utf8');
const candidates = ['ada_lovelace', 'grace_hopper', 'alan_turing'];

describe('ExtractResume', () => {
  it('reads the name and email', async () => {
    const ada = await b.ExtractResume(fixture('ada_lovelace'));
    expect(ada.name).toBe('Ada Lovelace');
    expect(ada.email).toBe('ada@analytical.engine');
  });

  it('lists skills for every candidate', async () => {
    for (const name of candidates) {
      const resume = await b.ExtractResume(fixture(name));
      expect(resume.skills.length, \`\${name}.txt\`).toBeGreaterThanOrEqual(3);
    }
  });

  it('orders experience newest first', async () => {
    const grace = await b.ExtractResume(fixture('grace_hopper'));
    const years = grace.experience.map((e) => e.start_year);
    expect(years).toEqual([...years].sort((a, b) => b - a));
  });
});
`;
