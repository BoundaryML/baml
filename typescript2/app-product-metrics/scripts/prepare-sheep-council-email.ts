import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createLoopsClient } from '../src/clients/loops.js';
import {
  assertSheepCouncilDraftReady,
  parseSheepCouncilEmailLmx,
  prepareSheepCouncilEmail,
} from '../src/sheep-council-email.js';

const defaultDraftPath = fileURLToPath(
  new URL('../emails/sheep-council-next.lmx', import.meta.url),
);
const usage = `Prepare or update the Loops draft for the next Sheep Council.

Usage:
  pnpm --filter app-product-metrics email:sheep-council -- [options]

Options:
  --file PATH           LMX source (defaults to emails/sheep-council-next.lmx)
  --apply               Create or update the Loops draft (default is dry-run)
  --help                Show this help
`;

interface CliOptions {
  apply: boolean;
  file: string;
}

function parseArgs(args: string[]): CliOptions | null {
  let apply = false;
  let file = defaultDraftPath;

  function readValue(index: number, flag: string): [string, number] {
    const argument = args[index];
    if (argument === undefined) throw new Error(`${flag} is missing`);
    const equalsIndex = argument.indexOf('=');
    if (equalsIndex >= 0) return [argument.slice(equalsIndex + 1), index];
    const value = args[index + 1];
    if (!value || value.startsWith('--')) {
      throw new Error(`${flag} requires a value`);
    }
    return [value, index + 1];
  }

  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === undefined || argument === '--') continue;
    if (argument === '--help') return null;
    if (argument === '--apply') {
      apply = true;
      continue;
    }
    const flag = argument.split('=', 1)[0];
    if (flag === '--file') {
      [file, index] = readValue(index, flag);
    } else {
      throw new Error(`Unknown argument: ${argument}`);
    }
  }
  return { apply, file: resolve(file) };
}

const options = parseArgs(process.argv.slice(2));
if (!options) {
  console.log(usage);
  process.exit(0);
}

const source = await readFile(options.file, 'utf8');
if (options.apply) assertSheepCouncilDraftReady(source);
const apiKey = process.env.LOOPS_EMAIL_CAMPAIGNS_API_KEY;
if (!apiKey) {
  throw new Error(
    'LOOPS_EMAIL_CAMPAIGNS_API_KEY is required; run this command through Infisical as documented in the README',
  );
}
const result = await prepareSheepCouncilEmail(
  createLoopsClient({
    apiKey,
    baseUrl: process.env.LOOPS_API_BASE_URL,
  }),
  parseSheepCouncilEmailLmx(source),
  options.apply,
);
console.log(JSON.stringify({ source: options.file, ...result }, null, 2));
if (!options.apply) {
  console.error('Dry-run only; pass --apply to write this draft to Loops.');
}
