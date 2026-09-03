import { readFile } from 'node:fs/promises';
import path from 'node:path';
import type { ReactNode } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { createMetadata } from '@/app/_lib/metadata';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { changeCount, parseChangelog, releaseId } from './changelog';
import styles from './page.module.css';

export const dynamic = 'force-static';

export const metadata = createMetadata({
  description: 'Release notes for the BAML language and toolchain.',
  ogSubtitle: 'What shipped in every BAML language release.',
  ogTitle: 'BAML language changelog',
  path: '/changelog',
  title: 'Changelog',
});

function formatDate(date: string): string {
  return new Intl.DateTimeFormat('en-US', {
    day: 'numeric',
    month: 'long',
    timeZone: 'UTC',
    year: 'numeric',
  }).format(new Date(`${date}T00:00:00Z`));
}

function categoryTone(label: string): string {
  if (label.toLowerCase().startsWith('breaking')) return styles.breaking;
  if (label.toLowerCase().startsWith('fix')) return styles.fixes;
  return styles.features;
}

function SectionHeading({ children }: { children?: ReactNode }) {
  const label = String(children);
  return (
    <h3 className={`${styles.sectionHeading} ${categoryTone(label)}`}>
      <span aria-hidden className={styles.sectionMark} />
      {children}
    </h3>
  );
}

export default async function ChangelogPage() {
  const source = await readFile(
    path.join(process.cwd(), 'data/changelog.md'),
    'utf8',
  );
  const releases = parseChangelog(source);
  const latest = releases[0];
  const totalChanges = releases.reduce(
    (total, release) => total + changeCount(release.body),
    0,
  );

  return (
    <>
      <Navbar />
      <main className={styles.page}>
        <div aria-hidden className={styles.grid} />
        <header className={styles.hero}>
          <p className={styles.eyebrow}>
            <span className={styles.pulse} />
            BAML language releases
          </p>
          <h1>What&rsquo;s new in BAML</h1>
          <p className={styles.intro}>
            New language features, compatibility notes, and fixes—straight from
            the source, with every pull request attached.
          </p>
          <div className={styles.heroMeta}>
            {latest && (
              <a
                className={styles.latestLink}
                href={`#${releaseId(latest.version)}`}
              >
                <span>Latest</span>v{latest.version}
                <span aria-hidden>↓</span>
              </a>
            )}
            <span className={styles.stat}>
              {releases.length} {releases.length === 1 ? 'release' : 'releases'}{' '}
              · {totalChanges} changes
            </span>
            <a
              className={styles.sourceLink}
              href="https://github.com/BoundaryML/baml/blob/canary/baml_language/CHANGELOG.md"
              rel="noreferrer"
              target="_blank"
            >
              View source <span aria-hidden>↗</span>
            </a>
          </div>
        </header>

        <div className={styles.layout}>
          <aside className={styles.releaseNav}>
            <p className={styles.navLabel}>Releases</p>
            <nav aria-label="Changelog releases">
              <ol>
                {releases.map((release, index) => (
                  <li key={release.version}>
                    <a
                      className={
                        index === 0 ? styles.currentRelease : undefined
                      }
                      href={`#${releaseId(release.version)}`}
                    >
                      <span>v{release.version}</span>
                      <time dateTime={release.date}>
                        {new Intl.DateTimeFormat('en-US', {
                          month: 'short',
                          timeZone: 'UTC',
                          year: 'numeric',
                        }).format(new Date(`${release.date}T00:00:00Z`))}
                      </time>
                    </a>
                  </li>
                ))}
              </ol>
            </nav>
          </aside>

          <div className={styles.releases}>
            {releases.map((release, index) => {
              const headingId = `${releaseId(release.version)}-heading`;
              return (
                <article
                  aria-labelledby={headingId}
                  className={`${styles.release} ${index === 0 ? styles.latestRelease : ''}`}
                  id={releaseId(release.version)}
                  key={release.version}
                >
                  <header className={styles.releaseHeader}>
                    <div>
                      <div className={styles.versionLine}>
                        {index === 0 && (
                          <span className={styles.latestBadge}>Latest</span>
                        )}
                        <span className={styles.versionPrefix}>Version</span>
                      </div>
                      <h2 id={headingId}>{release.version}</h2>
                    </div>
                    <div className={styles.releaseMeta}>
                      <time dateTime={release.date}>
                        {formatDate(release.date)}
                      </time>
                      <span>{changeCount(release.body)} changes</span>
                    </div>
                  </header>

                  <div className={styles.markdown}>
                    <ReactMarkdown
                      components={{
                        a: ({ children, href }) => (
                          <a
                            className={styles.markdownLink}
                            href={href}
                            rel="noreferrer"
                            target="_blank"
                          >
                            {children}
                          </a>
                        ),
                        code: ({ children }) => (
                          <code className={styles.inlineCode}>{children}</code>
                        ),
                        h3: SectionHeading,
                        li: ({ children }) => (
                          <li className={styles.changeItem}>{children}</li>
                        ),
                        ul: ({ children }) => (
                          <ul className={styles.changeList}>{children}</ul>
                        ),
                      }}
                      remarkPlugins={[remarkGfm]}
                    >
                      {release.body}
                    </ReactMarkdown>
                  </div>

                  <a
                    className={styles.compareLink}
                    href={release.compareUrl}
                    rel="noreferrer"
                    target="_blank"
                  >
                    Compare this release on GitHub <span aria-hidden>↗</span>
                  </a>
                </article>
              );
            })}
          </div>
        </div>
      </main>
      <FooterSection />
    </>
  );
}
