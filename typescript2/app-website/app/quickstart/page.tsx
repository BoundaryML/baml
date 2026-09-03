import { createMetadata } from '@/app/_lib/metadata';
// CONTENT PARITY: keep substantive copy and commands in sync with
// content/quickstart.md. Update both representations in the same change.
import { TryBaml } from '@/app/baml-intro/_components/try-baml';
import { DiscordCta } from '@/components/discord-cta';
import { EapCta } from '@/components/eap-cta';
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
/* One flex column with a uniform gap: spacing cannot collapse, inherit, or
   compound, so every block sits exactly 44px from its neighbors. */
.qs-wrap { margin: 0 auto; max-width: 720px; padding: 96px 24px 128px;
  display: flex; flex-direction: column; gap: 44px; }
.qs-wrap > * { margin: 0; }
.qs-head { display: flex; flex-direction: column; gap: 18px; }
.qs-sec { display: flex; flex-direction: column; gap: 14px; }
.qs-head > *,
.qs-sec > * { margin: 0; }
.qs-h1 { font-size: clamp(40px, 6vw, 56px); font-weight: 600; letter-spacing: -0.02em; line-height: 1; margin: 0 0 0 -0.045em; }
.qs-wrap h2 { font-size: 24px; font-weight: 600; letter-spacing: -0.01em; }
.qs-wrap p { color: #2b2b2b; font-size: 16px; line-height: 1.7; margin: 0; }
.qs-lead { font-size: 20px; font-weight: 400; line-height: 1.6; color: #1a1a1a; }
/* the install unit spans the column so it lines up with the cards below */
.qs-try > div { max-width: none; }
.qs-ctas { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.qs-wrap :not(pre) > code { background: rgba(0,0,0,0.06); border-radius: 4px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.86em; padding: 2px 5px; }
.qs-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; }
.qs-card { position: relative; display: flex; flex-direction: column; gap: 4px; padding: 13px 16px;
  border: 1px solid #e7e2d6; border-radius: 11px; background: #fffdf7; text-decoration: none;
  transition: transform 140ms ease, box-shadow 140ms ease, border-color 140ms ease; }
a.qs-card:hover { border-color: #cdbfa4; box-shadow: 0 5px 16px rgba(26,22,18,0.07); transform: translateY(-1px); }
.qs-card-t { font-size: 13px; font-weight: 600; color: #1a1a1a; letter-spacing: -0.005em; padding-right: 16px; }
.qs-card-v { font-size: 12px; line-height: 1.5; color: #6f6a63; word-break: break-word; }
a.qs-card .qs-card-v { color: #2563eb; }
a.qs-card::after { content: "↗"; position: absolute; top: 11px; right: 13px; font-size: 12px;
  color: #b4ae9f; transition: color 130ms ease, transform 130ms ease; }
a.qs-card:hover::after { color: #6d28d9; transform: translate(1px, -1px); }
@media (prefers-reduced-motion: reduce) {
  .qs-card, a.qs-card::after { transition: none; }
  a.qs-card:hover { transform: none; }
}
.qs-wrap .qs-note { font-size: 14px; line-height: 1.7; color: #6f6a63;
  border-left: 2px solid #d8cfbd; padding-left: 16px; }
@media (max-width: 560px) { .qs-grid, .qs-ctas { grid-template-columns: 1fr; } }
`;

export default function QuickstartPage() {
  return (
    <>
      {/* biome-ignore lint/security/noDangerouslySetInnerHtml: static page CSS */}
      <style dangerouslySetInnerHTML={{ __html: CSS }} />
      <Navbar />
      <main className="qs-wrap">
        <div className="qs-head">
          <h1 className="qs-h1">Quickstart</h1>
          <p className="qs-lead">
            BAML is the programming language for the AI era. BAML feels like
            typescript, but without many of the sins of javascript. It&rsquo;s
            incrementally adoptable, typesafe, and has agent first tooling.
          </p>
        </div>

        <div className="qs-try">
          <TryBaml />
        </div>

        <div className="qs-ctas">
          <EapCta />
          <DiscordCta />
        </div>

        <div className="qs-sec">
          <h2>Handy links</h2>
          <div className="qs-grid">
            <a className="qs-card" href="/explore">
              <span className="qs-card-t">Explore BAML</span>
              <span className="qs-card-v">see the language and code</span>
            </a>
            <a className="qs-card" href="/changelog">
              <span className="qs-card-t">Changelog</span>
              <span className="qs-card-v">language release notes</span>
            </a>
            <a
              className="qs-card"
              href="https://github.com/boundaryml/baml-demos"
              rel="noopener noreferrer"
              target="_blank"
            >
              <span className="qs-card-t">Demo repo</span>
              <span className="qs-card-v">
                github.com/boundaryml/baml-demos
              </span>
            </a>
            <a className="qs-card" href="/explore#describe">
              <span className="qs-card-t">Docs</span>
              <span className="qs-card-v">how baml describe works</span>
            </a>
          </div>
        </div>

        <div className="qs-sec">
          <h2>Keeping BAML updated</h2>
          <p className="qs-note">
            The <code>baml</code> wrapper is only a version manager, so it
            rarely needs updating. Update it through your package manager (
            <code>brew upgrade baml</code>) or with{' '}
            <code>baml self-update</code>. Everything here is discoverable from
            the binary itself, so agents shouldn&rsquo;t have any trouble. If
            they do, let us know.
          </p>
        </div>
      </main>
      <FooterSection />
    </>
  );
}
