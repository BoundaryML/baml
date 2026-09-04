import Link from 'next/link';

import type {
  GeneratedReleaseSnapshot,
  GeneratedReleaseSummary,
} from '@/lib/generated-content/build-content';

type GeneratedProduct = 'cli' | 'packages';

function productRoot(product: GeneratedProduct): string {
  return product === 'packages' ? '/baml/packages' : '/cli';
}

function productLabel(product: GeneratedProduct): string {
  return product === 'packages' ? 'standard packages' : 'CLI reference';
}

function formatReleaseDate(date: Date): string {
  return new Intl.DateTimeFormat('en-US', {
    dateStyle: 'medium',
    timeZone: 'UTC',
  }).format(date);
}

function channelText(release: GeneratedReleaseSummary): string {
  return release.channels.length > 0
    ? release.channels.join(', ')
    : 'historical';
}

function ReleaseLink({
  product,
  release,
}: {
  product: GeneratedProduct;
  release: GeneratedReleaseSummary;
}) {
  const href = `${productRoot(product)}/${release.routeVersion}`;
  return (
    <li>
      <Link href={href}>
        <code>{release.routeVersion}</code>
      </Link>{' '}
      <span className="text-muted-foreground">
        {channelText(release)} · published{' '}
        <time dateTime={release.release.released_at.toISOString()}>
          {formatReleaseDate(release.release.released_at)}
        </time>
      </span>
    </li>
  );
}

export function GeneratedReleaseCatalog({
  featured,
  product,
  releases,
}: {
  featured: GeneratedReleaseSnapshot | null;
  product: GeneratedProduct;
  releases: GeneratedReleaseSummary[];
}) {
  if (releases.length === 0) {
    return (
      <p>
        No complete generated releases are published yet. This page will list a
        release only after both its package and CLI records are available.
      </p>
    );
  }

  const currentReleases = releases.filter(
    (release) => release.channels.length > 0,
  );
  const displayedCurrentReleases =
    currentReleases.length > 0 ? currentReleases : releases.slice(0, 1);
  const hasStable = currentReleases.some((release) =>
    release.channels.includes('stable'),
  );

  return (
    <>
      <h3>{hasStable ? 'Current stable release' : 'Current snapshots'}</h3>
      {!hasStable ? (
        <p>
          No stable release has been published in this catalog. Canary and
          nightly entries are labeled explicitly and are not stable releases.
        </p>
      ) : null}
      <ul>
        {displayedCurrentReleases.map((release) => (
          <ReleaseLink
            key={release.routeVersion}
            product={product}
            release={release}
          />
        ))}
      </ul>

      {featured && product === 'packages' ? (
        <>
          <h3>
            Package catalog for <code>{featured.routeVersion}</code>
          </h3>
          <ul className="columns-2 sm:columns-3">
            {featured.pages
              .filter((page) => page.page_kind === 'package')
              .map((page) => (
                <li key={page.route_path}>
                  <Link
                    href={`/baml/packages/${featured.routeVersion}/${page.route_path}`}
                  >
                    <code>{page.qualified_name}</code>
                  </Link>
                </li>
              ))}
          </ul>
        </>
      ) : null}

      {featured && product === 'cli' ? (
        <p>
          Browse the{' '}
          <Link href={`/cli/${featured.routeVersion}`}>
            {featured.routeVersion} CLI overview
          </Link>{' '}
          or open its{' '}
          <Link href={`/cli/${featured.routeVersion}/commands`}>
            complete command index
          </Link>
          .
        </p>
      ) : null}

      <h3>All published versions</h3>
      <p>
        Every link below opens immutable {productLabel(product)} generated from
        that exact BAML toolchain release.
      </p>
      <ul>
        {releases.map((release) => (
          <ReleaseLink
            key={release.routeVersion}
            product={product}
            release={release}
          />
        ))}
      </ul>
    </>
  );
}
