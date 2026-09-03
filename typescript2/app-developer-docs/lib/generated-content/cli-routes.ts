import type { CliCommandNodeInput } from '@/lib/generated-content/schemas';

export function flattenCliCommands(
  root: CliCommandNodeInput,
): CliCommandNodeInput[] {
  const commands: CliCommandNodeInput[] = [];
  const visit = (command: CliCommandNodeInput): void => {
    if (command.command_path.length > 0) commands.push(command);
    for (const child of command.subcommands) visit(child);
  };
  visit(root);
  return commands;
}

export function findCliCommand(
  root: CliCommandNodeInput,
  commandPath: readonly string[],
): CliCommandNodeInput | null {
  let current = root;
  for (const token of commandPath) {
    const child = current.subcommands.find(
      (candidate) => candidate.name === token,
    );
    if (!child) return null;
    current = child;
  }
  return current;
}
