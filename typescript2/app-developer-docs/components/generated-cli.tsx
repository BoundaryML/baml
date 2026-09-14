import Link from 'next/link';

import type { CliCommandNodeInput } from '@/lib/generated-content/schemas';

export function cliCommandHref(
  routeVersion: string,
  commandPath: readonly string[],
): string {
  return `/cli/${routeVersion}/commands/${commandPath.join('/')}`;
}

function flagAnchor(flag: CliCommandNodeInput['flags'][number]): string {
  return (flag.long ?? flag.short ?? 'flag').replace(/^-+/, '');
}

export function CliCommandTree({
  commands,
  routeVersion,
}: {
  commands: CliCommandNodeInput[];
  routeVersion: string;
}) {
  return (
    <ul>
      {commands.map((command) => (
        <li key={command.command_path.join('\0')}>
          <Link href={cliCommandHref(routeVersion, command.command_path)}>
            <code>baml {command.command_path.join(' ')}</code>
          </Link>
          {command.description ? ` — ${command.description}` : null}
          {command.subcommands.length > 0 ? (
            <CliCommandTree
              commands={command.subcommands}
              routeVersion={routeVersion}
            />
          ) : null}
        </li>
      ))}
    </ul>
  );
}

export function CliCommandContent({
  command,
  routeVersion,
}: {
  command: CliCommandNodeInput;
  routeVersion: string;
}) {
  return (
    <>
      <section>
        <h2 id="usage">Usage</h2>
        <pre>
          <code>{command.usage}</code>
        </pre>
      </section>
      {command.subcommands.length > 0 ? (
        <section>
          <h2 id="subcommands">Subcommands</h2>
          <CliCommandTree
            commands={command.subcommands}
            routeVersion={routeVersion}
          />
        </section>
      ) : null}
      {command.arguments.length > 0 ? (
        <section>
          <h2 id="arguments">Arguments</h2>
          {command.arguments.map((argument) => (
            <article className="border-t py-4" key={argument.name}>
              <h3>
                <code>{argument.name}</code>
                {argument.required ? ' (required)' : null}
              </h3>
              {argument.description ? <p>{argument.description}</p> : null}
              {argument.default_value ? (
                <p>
                  Default: <code>{argument.default_value}</code>
                </p>
              ) : null}
              {argument.allowed_values.length > 0 ? (
                <p>
                  Allowed values:{' '}
                  <code>{argument.allowed_values.join(', ')}</code>
                </p>
              ) : null}
            </article>
          ))}
        </section>
      ) : null}
      {command.flags.length > 0 ? (
        <section>
          <h2 id="options">Options</h2>
          {command.flags.map((flag) => (
            <article
              className="scroll-mt-24 border-t py-4"
              id={flagAnchor(flag)}
              key={`${flag.short ?? ''}-${flag.long ?? ''}`}
            >
              <h3>
                <code>
                  {[flag.short, flag.long, flag.value_name]
                    .filter(Boolean)
                    .join(', ')}
                </code>
              </h3>
              {flag.description ? <p>{flag.description}</p> : null}
              {flag.default_value ? (
                <p>
                  Default: <code>{flag.default_value}</code>
                </p>
              ) : null}
              {flag.allowed_values.length > 0 ? (
                <p>
                  Allowed values: <code>{flag.allowed_values.join(', ')}</code>
                </p>
              ) : null}
            </article>
          ))}
        </section>
      ) : null}
    </>
  );
}
