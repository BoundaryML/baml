const EXACT_VERSION_PATTERN = /^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$/;

export function canonicalVersionToRouteVersion(version: string): string {
  if (!EXACT_VERSION_PATTERN.test(version)) {
    throw new Error(`Invalid canonical documentation version: ${version}.`);
  }
  return `v${version}`;
}

export function routeVersionToCanonicalVersion(
  routeVersion: string,
): string | null {
  if (!routeVersion.startsWith('v')) return null;
  const version = routeVersion.slice(1);
  return EXACT_VERSION_PATTERN.test(version) ? version : null;
}

export function isPrereleaseVersion(version: string): boolean {
  return version.includes('-');
}
