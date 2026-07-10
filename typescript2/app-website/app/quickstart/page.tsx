import { createMetadata } from '@/app/_lib/metadata';
import { TryBaml } from '@/app/baml-intro/_components/TryBaml';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';

export const metadata = createMetadata({
  description:
    'Install BAML and its toolchain, agent skills, and editor extensions.',
  ogSubtitle: 'Mac, Windows, and Linux.',
  ogTitle: 'Install BAML in seconds',
  path: '/quickstart',
  title: 'Quickstart',
});

const CSS = `
.qs-wrap { margin: 0 auto; max-width: 720px; padding: 96px 24px 128px; }
.qs-h1 { font-size: clamp(40px, 6vw, 56px); font-weight: 600; letter-spacing: -0.02em; line-height: 1; margin: 0 0 48px; }
.qs-wrap h2 { font-size: 24px; font-weight: 600; letter-spacing: -0.01em; margin: 48px 0 16px; }
.qs-wrap h2:first-of-type { margin-top: 0; }
.qs-wrap p { color: #2b2b2b; font-size: 16px; line-height: 1.7; margin: 16px 0; }
.qs-lead { font-size: 19px; font-weight: 600; line-height: 1.55; color: #1a1a1a; margin: 0 0 8px; }
.qs-wrap :not(pre) > code { background: rgba(0,0,0,0.06); border-radius: 4px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.86em; padding: 2px 5px; }
.qs-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; margin: 8px 0 0; }
.qs-card { display: flex; flex-direction: column; gap: 3px; padding: 12px 14px;
  border: 1px solid #e7e2d6; border-radius: 10px; background: #fffdf7; text-decoration: none;
  transition: border-color 120ms ease, background 120ms ease; }
a.qs-card:hover { border-color: #d8cfbd; background: #fbf7ed; }
.qs-card-t { font-size: 13px; font-weight: 600; color: #1a1a1a; }
.qs-card-v { font-size: 12.5px; line-height: 1.5; color: #6f6a63; word-break: break-word; }
a.qs-card .qs-card-v { color: #2563eb; }
.qs-wrap .qs-note { font-size: 14px; line-height: 1.7; color: #6f6a63;
  border-left: 2px solid #d8cfbd; padding-left: 16px; margin: 8px 0 0; }
@media (max-width: 560px) { .qs-grid { grid-template-columns: 1fr; } }
`;

export default function QuickstartPage() {
  return (
    <>
      {/* eslint-disable-next-line react/no-danger */}
      <style dangerouslySetInnerHTML={{ __html: CSS }} />
      <Navbar />
      <main className="qs-wrap">
        <h1 className="qs-h1">Quickstart</h1>

        <p className="qs-lead">
          BAML is the programming language for the AI era. BAML feels like
          typescript, but without many of the sins of javascript. It&rsquo;s
          incrementally adoptable, typesafe, and has agent first tooling.
        </p>

        <TryBaml />

        <h2>Handy links</h2>
        <div className="qs-grid">
          <a className="qs-card" href="https://new.boundaryml.com/changelog">
            <span className="qs-card-t">Changelog</span>
            <span className="qs-card-v">new.boundaryml.com/changelog</span>
          </a>
          <a
            className="qs-card"
            href="https://github.com/boundaryml/baml-demos"
            rel="noopener noreferrer"
            target="_blank"
          >
            <span className="qs-card-t">Demo repo</span>
            <span className="qs-card-v">github.com/boundaryml/baml-demos</span>
          </a>
          <a
            className="qs-card"
            href="https://www.boundaryml.com/discord"
            rel="noopener noreferrer"
            target="_blank"
          >
            <span className="qs-card-t">Discord</span>
            <span className="qs-card-v">boundaryml.com/discord</span>
          </a>
          <a className="qs-card" href="https://new.boundaryml.com/eap">
            <span className="qs-card-t">Early access</span>
            <span className="qs-card-v">new.boundaryml.com/eap</span>
          </a>
          <div className="qs-card">
            <span className="qs-card-t">Docs</span>
            <span className="qs-card-v">
              no docs, just run <code>baml describe</code>
            </span>
          </div>
          <div className="qs-card">
            <span className="qs-card-t">Agent setup</span>
            <span className="qs-card-v">
              <code>baml agent install</code>
            </span>
          </div>
        </div>

        <h2>Keeping it updated</h2>
        <p className="qs-note">
          The <code>baml</code> wrapper is only a version manager, so it rarely
          needs updating. Update it through your package manager (
          <code>brew upgrade baml</code>) or with <code>baml self-update</code>.
          Everything here is discoverable from the binary itself, so agents
          shouldn&rsquo;t have any trouble. If they do, let us know.
        </p>
      </main>
      <FooterSection />
    </>
  );
}
