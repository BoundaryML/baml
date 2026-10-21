export interface ParsedOperatorArguments {
  flags: Set<string>;
  values: Map<string, string>;
}

export function parseOperatorArguments(
  argumentsToParse: readonly string[],
  allowedValues: readonly string[],
  allowedFlags: readonly string[],
): ParsedOperatorArguments {
  const valueNames = new Set(allowedValues);
  const flagNames = new Set(allowedFlags);
  const values = new Map<string, string>();
  const flags = new Set<string>();

  for (let index = 0; index < argumentsToParse.length; index += 1) {
    const argument = argumentsToParse[index];
    if (!argument.startsWith('--')) {
      throw new Error(`Unexpected positional argument: ${argument}.`);
    }
    const name = argument.slice(2);
    if (flagNames.has(name)) {
      if (flags.has(name)) {
        throw new Error(`Duplicate flag: --${name}.`);
      }
      flags.add(name);
      continue;
    }
    if (!valueNames.has(name)) {
      throw new Error(`Unknown option: --${name}.`);
    }
    if (values.has(name)) {
      throw new Error(`Duplicate option: --${name}.`);
    }
    const value = argumentsToParse[index + 1];
    if (!value || value.startsWith('--')) {
      throw new Error(`Option --${name} requires a value.`);
    }
    values.set(name, value);
    index += 1;
  }

  return { flags, values };
}

export function requireOperatorValue(
  argumentsToRead: ParsedOperatorArguments,
  name: string,
): string {
  const value = argumentsToRead.values.get(name);
  if (!value) {
    throw new Error(`Missing required option: --${name}.`);
  }
  return value;
}
