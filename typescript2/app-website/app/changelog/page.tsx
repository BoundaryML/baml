import { createMetadata } from '@/app/_lib/metadata';
// CONTENT PARITY: keep the page introduction in sync with
// content/changelog.md. Live entries are appended by the Markdown route.
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';
import { ChangelogList } from './changelog-list';
import { fetchChangelogEntries } from './feed';

// This page is prerendered with ISR (`revalidate = 60`), NOT force-dynamic, so
// it keeps the never-500 property: visitors always get CDN-cached HTML with the
// release list already rendered, and Convex is only consulted during background
// regeneration — if that fails, the stale page keeps serving. Entry BODIES are
// deliberately left out of the list payload (they dwarf everything else);
// clicking a release opens its article via a `?v=` query param and the client
// fetches the body from `/api/changelog-feed/entries/[version]`. Code blocks
// are syntax-highlighted client-side with shiki (incl. the BAML grammar).

export const revalidate = 60;

export const metadata = createMetadata({
  description: 'The latest releases of BAML, shipped continuously.',
  ogTitle: 'Changelog',
  path: '/changelog',
  title: 'Changelog',
});

const CSS = `
.chlog-wrap { margin: 0 auto; max-width: 760px; padding: 96px 24px 128px; }

/* list header */
.chlog-header { margin-bottom: 48px; }
.chlog-h1 { font-size: clamp(40px, 6vw, 60px); font-weight: 600; letter-spacing: -0.02em; line-height: 1; margin: 0; }
.chlog-sub { color: #6b6456; font-size: 18px; margin: 20px 0 0; max-width: 460px; }

/* channel filter bar */
.chlog-filters { display: flex; flex-wrap: wrap; gap: 8px; margin: 0 0 28px; }
.chlog-filter { border: 1px solid #e0dac9; background: transparent; border-radius: 999px;
  padding: 5px 14px; font-size: 13px; font-weight: 500; color: #6b6456; cursor: pointer;
  transition: border-color 0.12s ease, color 0.12s ease, background 0.12s ease; }
.chlog-filter:hover { border-color: #c9c2af; color: #1a1a1a; }
.chlog-filter.is-active { background: #1a1a1a; border-color: #1a1a1a; color: #fff; }

/* channel tags */
.chlog-tag { display: inline-block; border-radius: 999px; font-size: 11px; font-weight: 600;
  letter-spacing: 0.02em; line-height: 1.5; padding: 1px 8px; white-space: nowrap; }
.chlog-tag--stable { color: #1a7f37; background: rgba(26,127,55,0.12); }
.chlog-tag--nightly { color: #4338ca; background: rgba(67,56,202,0.12); }
.chlog-tag--canary { color: #b45309; background: rgba(180,83,9,0.12); }
.chlog-tag--alpha { color: #7c3aed; background: rgba(124,58,237,0.12); }
.chlog-tag--prerelease { color: #6b6456; background: rgba(0,0,0,0.07); }

/* timeline of releases */
.chlog-timeline { list-style: none; margin: 0; padding: 0; position: relative; }
.chlog-rail { position: absolute; top: 8px; bottom: 8px; left: 140px; width: 1px; background: #e7e2d6; }
.chlog-tl-item { position: relative; display: grid; grid-template-columns: 120px 40px 1fr;
  align-items: start; padding: 36px 0; }
.chlog-tl-item:first-child { padding-top: 4px; }
.chlog-tl-date { grid-column-start: 1; text-align: right; padding-top: 4px; white-space: nowrap;
  color: #8a8372; font-size: 12px; font-weight: 500; }
.chlog-tl-dot { grid-column-start: 2; justify-self: center; margin-top: 6px; position: relative; z-index: 1;
  display: flex; align-items: center; justify-content: center; width: 12px; height: 12px;
  border: 1px solid #d4cebd; border-radius: 50%; background: #FBF7ED; }
.chlog-tl-dot > span { width: 4px; height: 4px; border-radius: 50%; background: #1a1a1a; }
.chlog-tl-content { grid-column-start: 3; min-width: 0; }
.chlog-tl-head { display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap; }
.chlog-tl-title { text-align: left; background: none; border: 0; padding: 0; cursor: pointer;
  color: #1a1a1a; font-size: 20px; font-weight: 600; letter-spacing: -0.01em; line-height: 1.25; }
.chlog-tl-title:hover { text-decoration: underline; }
.chlog-tl-lede { color: #6b6456; font-size: 15px; line-height: 1.6; margin: 8px 0 0;
  display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical; overflow: hidden; }
.chlog-tl-read { background: none; border: 0; padding: 0; margin: 12px 0 0; cursor: pointer;
  color: #2563eb; font-size: 13px; font-weight: 500; }
.chlog-tl-read:hover { text-decoration: underline; }

/* article view */
.chlog-back { background: none; border: 0; color: #6b6456; cursor: pointer; font-size: 13px;
  margin: 0 0 32px; padding: 0; }
.chlog-back:hover { color: #1a1a1a; }
.chlog-meta { align-items: center; color: #8a8372; display: flex; font-size: 12px; gap: 12px; margin: 0 0 10px; }
.chlog-ver { background: rgba(0,0,0,0.05); border-radius: 4px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; padding: 2px 7px; }
.chlog-article-title { font-size: clamp(28px, 5vw, 40px); font-weight: 600; letter-spacing: -0.02em; line-height: 1.1; margin: 0 0 32px; }
.chlog-authors { color: #8a8372; font-size: 13px; margin-top: 40px; border-top: 1px solid #e7e2d6; padding-top: 20px; }

/* markdown body */
.chlog-md { color: #1a1a1a; font-size: 16px; line-height: 1.7; }
.chlog-md > :first-child { margin-top: 0; }
.chlog-md p { margin: 16px 0; }
.chlog-md h2 { font-size: 22px; font-weight: 600; margin: 36px 0 12px; letter-spacing: -0.01em; }
.chlog-md h3 { font-size: 17px; font-weight: 600; margin: 28px 0 10px; }
.chlog-md ul, .chlog-md ol { margin: 16px 0; padding-left: 1.4em; }
.chlog-md li { margin: 8px 0; }
.chlog-md a { color: #2563eb; text-decoration: none; }
.chlog-md a:hover { text-decoration: underline; }
.chlog-md :not(pre) > code { background: rgba(0,0,0,0.06); border-radius: 4px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.86em; padding: 2px 5px; }
.chlog-md pre { border: 1px solid #e7e2d6; border-radius: 10px; font-size: 13.5px; line-height: 1.6;
  margin: 20px 0; overflow-x: auto; padding: 16px 18px; }
.chlog-md pre code { background: none; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; padding: 0; }
.chlog-md blockquote { border-left: 3px solid #e7e2d6; color: #6b6456; margin: 16px 0; padding-left: 16px; }
.chlog-md table { border-collapse: collapse; font-size: 14px; margin: 20px 0; width: 100%; }
.chlog-md th, .chlog-md td { border: 1px solid #e7e2d6; padding: 8px 12px; text-align: left; }
`;

// First paragraph of the markdown body, flattened to plain text, for the list
// preview. Strips fenced code, headings, list markers, and inline markdown.
function lede(body: string): string {
  const withoutCode = body.replace(/```[\s\S]*?```/g, '');
  const para =
    withoutCode
      .split(/\n\s*\n/)
      .map((p) => p.trim())
      .find((p) => p && !p.startsWith('#')) ?? '';
  return para
    .replace(/^[#>\-*\s]+/, '')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]+\)/g, '$1')
    .replace(/\s+/g, ' ')
    .trim();
}

export default async function ChangelogPage() {
  // Degrade to an empty list rather than failing the render: a fresh build
  // with Convex down shows "No entries yet." and self-heals on revalidation.
  const entries = await fetchChangelogEntries({ revalidate: 60 }).catch(
    () => [],
  );
  const listEntries = entries.map((e) => ({
    date: e.date,
    lede: lede(e.body),
    title: e.title,
    version: e.version,
  }));

  return (
    <>
      {/* eslint-disable-next-line react/no-danger */}
      <style dangerouslySetInnerHTML={{ __html: CSS }} />
      <Navbar />
      <main className="chlog-wrap">
        <ChangelogList entries={listEntries} />
      </main>
      <FooterSection />
    </>
  );
}
