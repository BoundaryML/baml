const postgresEnvironmentVariables = [
  'POSTGRES_OPSBOT_HOST',
  'POSTGRES_OPSBOT_PORT',
  'POSTGRES_OPSBOT_DATABASE',
  'POSTGRES_OPSBOT_USERNAME',
  'POSTGRES_OPSBOT_PASSWORD',
] as const;

type Environment = Record<string, string | undefined>;

function requiredValue(environment: Environment, name: string): string {
  const value = environment[name];
  if (!value?.trim()) throw new Error(`${name} is required`);
  return value;
}

export function optionalPostgresConnectionUrl(
  environment: Environment,
): string | undefined {
  const configured = postgresEnvironmentVariables.filter((name) =>
    environment[name]?.trim(),
  );
  if (configured.length === 0) return undefined;
  if (configured.length !== postgresEnvironmentVariables.length) {
    const missing = postgresEnvironmentVariables.filter(
      (name) => !environment[name]?.trim(),
    );
    throw new Error(
      `Incomplete Postgres configuration; missing ${missing.join(', ')}`,
    );
  }
  return postgresConnectionUrl(environment);
}

export function postgresConnectionUrl(environment: Environment): string {
  const host = requiredValue(environment, 'POSTGRES_OPSBOT_HOST');
  if (host.includes('/') || host.includes(':')) {
    throw new Error(
      'POSTGRES_OPSBOT_HOST must be a hostname without a scheme or port',
    );
  }
  const portText = requiredValue(environment, 'POSTGRES_OPSBOT_PORT');
  const port = Number(portText);
  if (!Number.isInteger(port) || port < 1 || port > 65_535) {
    throw new Error(
      'POSTGRES_OPSBOT_PORT must be an integer between 1 and 65535',
    );
  }
  const database = requiredValue(environment, 'POSTGRES_OPSBOT_DATABASE');
  if (database.includes('/')) {
    throw new Error('POSTGRES_OPSBOT_DATABASE must not contain a slash');
  }
  const url = new URL('postgresql://localhost');
  url.hostname = host;
  url.port = portText;
  url.username = requiredValue(environment, 'POSTGRES_OPSBOT_USERNAME');
  url.password = requiredValue(environment, 'POSTGRES_OPSBOT_PASSWORD');
  url.pathname = `/${database}`;
  url.searchParams.set('sslmode', 'verify-full');
  return url.toString();
}
