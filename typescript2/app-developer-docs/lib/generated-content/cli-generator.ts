import {
  invokeBaml,
  readBamlBinaryIdentity,
} from '@/lib/generated-content/baml-binary';
import { hashCliSource } from '@/lib/generated-content/cli-source';
import { CLI_ARTIFACT_SCHEMA_VERSION } from '@/lib/generated-content/constants';
import {
  canonicalJson,
  jsonValueSchema,
  sha256,
} from '@/lib/generated-content/json';
import {
  type CliArtifactPayload,
  type CliCommandNodeInput,
  cliArtifactPayloadSchema,
  cliCommandNodeSchema,
} from '@/lib/generated-content/schemas';

export interface CliPublicationInput {
  artifactSchemaVersion: number;
  payload: CliArtifactPayload;
  payloadJson: string;
  payloadSha256: string;
  productVersion: string;
  sourceSha256: string;
  wrapperVersion: string;
}

interface HelpRow {
  description: string;
  specification: string;
}

interface ParsedHelp {
  arguments: CliCommandNodeInput['arguments'];
  commandRows: HelpRow[];
  description: string | null;
  flags: CliCommandNodeInput['flags'];
  usage: string;
}

function findSection(lines: string[], names: readonly string[]): string[] {
  const start = lines.findIndex((line) => names.includes(line.trim()));
  if (start === -1) {
    return [];
  }
  const output: string[] = [];
  for (const line of lines.slice(start + 1)) {
    if (/^[A-Za-z][A-Za-z ]+:\s*$/.test(line)) {
      break;
    }
    if (line.trim().length === 0 && output.length > 0) {
      break;
    }
    output.push(line);
  }
  return output;
}

function parseHelpRows(lines: string[]): HelpRow[] {
  const rows: HelpRow[] = [];
  for (const line of lines) {
    if (line.trim().length === 0) {
      continue;
    }
    const match = /^\s{2,}(\S.*?)\s{2,}(\S.*)$/.exec(line);
    if (match) {
      rows.push({
        description: match[2].trim(),
        specification: match[1].trim(),
      });
      continue;
    }
    if (rows.length > 0 && /^\s{4,}\S/.test(line)) {
      const previousRow = rows.at(-1);
      if (!previousRow) {
        throw new Error(
          `Unable to associate emitted help continuation: ${line}.`,
        );
      }
      previousRow.description = [previousRow.description, line.trim()]
        .filter(Boolean)
        .join(' ');
      continue;
    }
    if (/^\s{2,}\S/.test(line)) {
      rows.push({ description: '', specification: line.trim() });
      continue;
    }
    throw new Error(`Unable to parse emitted help row: ${line}.`);
  }
  return rows;
}

function parseAllowedValues(description: string): string[] {
  const match = /\[possible values: ([^\]]+)\]/.exec(description);
  return match ? match[1].split(',').map((value) => value.trim()) : [];
}

function parseDefaultValue(description: string): string | null {
  return /\[default: ([^\]]+)\]/.exec(description)?.[1] ?? null;
}

function parseUsage(lines: string[]): string {
  const usageIndex = lines.findIndex((line) => line.startsWith('Usage:'));
  if (usageIndex === -1) {
    throw new Error('Emitted CLI help does not contain a Usage section.');
  }
  const inline = lines[usageIndex].slice('Usage:'.length).trim();
  const parts = inline ? [inline] : [];
  for (const line of lines.slice(usageIndex + 1)) {
    if (line.trim().length === 0) {
      break;
    }
    if (!/^\s+/.test(line)) {
      break;
    }
    parts.push(line.trim());
  }
  if (parts.length === 0) {
    throw new Error('Emitted CLI help contains an empty Usage section.');
  }
  return parts.join(' ');
}

function parseDescription(lines: string[]): string | null {
  const usageIndex = lines.findIndex((line) => line.startsWith('Usage:'));
  const description = lines
    .slice(0, usageIndex)
    .map((line) => line.trim())
    .filter(Boolean)
    .join(' ');
  return description || null;
}

function parseArguments(rows: HelpRow[]): CliCommandNodeInput['arguments'] {
  return rows.map((row) => ({
    allowed_values: parseAllowedValues(row.description),
    default_value: parseDefaultValue(row.description),
    description: row.description || null,
    name: row.specification,
    required: row.specification.startsWith('<'),
  }));
}

function parseFlags(rows: HelpRow[]): CliCommandNodeInput['flags'] {
  return rows.map((row) => {
    const short =
      /(?:^|,\s*)(-[A-Za-z?])(?:,|\s|$)/.exec(row.specification)?.[1] ?? null;
    const long =
      /(?:^|,\s*)(--[A-Za-z0-9][A-Za-z0-9-]*)(?:\.{3}|\s|$)/.exec(
        row.specification,
      )?.[1] ?? null;
    const valueName =
      /(<[^>]+>|\[[^\]]+\])/.exec(row.specification)?.[1] ?? null;
    if (!short && !long) {
      throw new Error(
        `Unable to parse emitted CLI flag: ${row.specification}.`,
      );
    }
    return {
      allowed_values: parseAllowedValues(row.description),
      default_value: parseDefaultValue(row.description),
      description: row.description || null,
      long,
      short,
      value_name: valueName,
    };
  });
}

function parseHelp(text: string): ParsedHelp {
  const lines = text.replaceAll('\r\n', '\n').split('\n');
  return {
    arguments: parseArguments(
      parseHelpRows(findSection(lines, ['Arguments:'])),
    ),
    commandRows: parseHelpRows(findSection(lines, ['Commands:'])),
    description: parseDescription(lines),
    flags: parseFlags(
      parseHelpRows(findSection(lines, ['Options:', 'Global options:'])),
    ),
    usage: parseUsage(lines),
  };
}

function commandNameFromSpecification(specification: string): string {
  const name = specification.split(/\s+/, 1)[0];
  if (!/^[a-z][a-z0-9-]*$/.test(name)) {
    throw new Error(
      `Unable to derive a public command name from ${specification}.`,
    );
  }
  return name;
}

function leafFromHelpRow(
  parentPath: string[],
  row: HelpRow,
): CliCommandNodeInput {
  const name = commandNameFromSpecification(row.specification);
  return {
    arguments: [],
    command_path: [...parentPath, name],
    description: row.description || null,
    flags: [],
    name,
    subcommands: [],
    usage: `baml ${[...parentPath, row.specification].join(' ')}`,
  };
}

export async function generateCliPublicationInput(
  bamlBinary: string,
): Promise<CliPublicationInput> {
  const identity = await readBamlBinaryIdentity(bamlBinary);
  const rawHelp: CliArtifactPayload['raw_help'] = [];
  const visited = new Set<string>();

  const capture = async (
    commandPath: string[],
    invocation: string[],
    recurse: boolean,
  ): Promise<CliCommandNodeInput> => {
    const pathKey = commandPath.join('\0');
    if (visited.has(pathKey)) {
      throw new Error(`CLI command traversal cycle: ${commandPath.join(' ')}.`);
    }
    visited.add(pathKey);
    const result = await invokeBaml(bamlBinary, invocation);
    const parsed = parseHelp(result.stdout);
    rawHelp.push({
      command_path: commandPath,
      invocation,
      sha256: sha256(result.stdout),
      text: result.stdout,
    });
    const name = commandPath.at(-1) ?? 'baml';
    const subcommands = recurse
      ? await Promise.all(
          parsed.commandRows
            .filter(
              (row) =>
                commandNameFromSpecification(row.specification) !== 'help',
            )
            .map((row) => {
              const childName = commandNameFromSpecification(row.specification);
              const childPath = [...commandPath, childName];
              return capture(childPath, ['help', ...childPath], true);
            }),
        )
      : parsed.commandRows.map((row) => leafFromHelpRow(commandPath, row));
    return cliCommandNodeSchema.parse({
      arguments: parsed.arguments,
      command_path: commandPath,
      description: parsed.description,
      flags: parsed.flags,
      name,
      subcommands,
      usage: parsed.usage,
    });
  };

  const root = await capture([], ['help'], true);
  const toolchain = await capture(['toolchain'], ['toolchain', 'help'], false);
  const selfUpdate = await capture(
    ['self-update'],
    ['self-update', '--help'],
    false,
  );
  root.subcommands.push(toolchain, selfUpdate);
  root.subcommands.sort((left, right) => left.name.localeCompare(right.name));

  const payload = cliArtifactPayloadSchema.parse({
    artifact_schema_version: CLI_ARTIFACT_SCHEMA_VERSION,
    product_version: identity.productVersion,
    raw_help: rawHelp.sort((left, right) =>
      left.command_path.join('\0').localeCompare(right.command_path.join('\0')),
    ),
    root,
    wrapper_version: identity.wrapperVersion,
  });
  const payloadJson = canonicalJson(jsonValueSchema.parse(payload));
  return {
    artifactSchemaVersion: CLI_ARTIFACT_SCHEMA_VERSION,
    payload,
    payloadJson,
    payloadSha256: sha256(payloadJson),
    productVersion: identity.productVersion,
    sourceSha256: hashCliSource(payload.raw_help),
    wrapperVersion: identity.wrapperVersion,
  };
}
