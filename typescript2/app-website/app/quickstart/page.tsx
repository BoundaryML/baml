import type { Metadata } from 'next';
import { codeToHtml } from 'shiki';

import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';

// Static page: the content is hardcoded, so code blocks are highlighted at
// BUILD time with shiki (server-side, full highlighting). The page is
// prerendered to plain HTML — no client-side shiki, no serverless function,
// no 500 risk.

const baseUrl =
  process.env.NEXT_PUBLIC_BASE_URL ??
  (process.env.VERCEL_PROJECT_PRODUCTION_URL
    ? `https://${process.env.VERCEL_PROJECT_PRODUCTION_URL}`
    : 'https://boundaryml.com');

export const metadata: Metadata = {
  alternates: { canonical: `${baseUrl}/quickstart` },
  description:
    'Install BAML and its toolchain, agent skills, and editor extensions.',
  openGraph: {
    description:
      'Install BAML and its toolchain, agent skills, and editor extensions.',
    siteName: 'BAML',
    title: 'BAML Quickstart',
    type: 'website',
    url: `${baseUrl}/quickstart`,
  },
  title: 'Quickstart | BAML',
};

/** Build-time syntax-highlighted code block. */
async function Code({ children, lang = 'bash' }: { children: string; lang?: string }) {
  const html = await codeToHtml(children.trim(), {
    lang,
    theme: 'github-light',
  });
  // eslint-disable-next-line react/no-danger
  return <div className="qs-code" dangerouslySetInnerHTML={{ __html: html }} />;
}

const CSS = `
.qs-wrap { margin: 0 auto; max-width: 720px; padding: 96px 24px 128px; }
.qs-h1 { font-size: clamp(40px, 6vw, 56px); font-weight: 600; letter-spacing: -0.02em; line-height: 1; margin: 0 0 48px; }
.qs-wrap h2 { font-size: 24px; font-weight: 600; letter-spacing: -0.01em; margin: 48px 0 16px; }
.qs-wrap h2:first-of-type { margin-top: 0; }
.qs-wrap p { color: #2b2b2b; font-size: 16px; line-height: 1.7; margin: 16px 0; }
.qs-lead { font-size: 19px; font-weight: 600; line-height: 1.55; color: #1a1a1a; margin: 0 0 8px; }
.qs-links { margin: 8px 0 0; padding-left: 1.2em; }
.qs-links li { color: #2b2b2b; font-size: 16px; line-height: 1.7; margin: 6px 0; }
.qs-links a { color: #2563eb; text-decoration: none; word-break: break-word; }
.qs-links a:hover { text-decoration: underline; }
.qs-wrap :not(pre) > code { background: rgba(0,0,0,0.06); border-radius: 4px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.86em; padding: 2px 5px; }
.qs-code { margin: 16px 0; }
.qs-code pre { border: 1px solid #e7e2d6; border-radius: 10px; font-size: 14px; line-height: 1.6;
  margin: 0; overflow-x: auto; padding: 14px 18px; }
.qs-code pre code { background: none; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; padding: 0; }
.qs-details { border: 1px solid #e7e2d6; border-radius: 10px; margin: 16px 0; padding: 4px 16px; }
.qs-details > summary { cursor: pointer; font-size: 15px; font-weight: 500; list-style: revert;
  padding: 10px 0; color: #2b2b2b; }
.qs-details[open] > summary { margin-bottom: 4px; }
.qs-details .qs-code { margin: 0 0 12px; }
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

        <h2>Installing BAML and Toolchain</h2>
        <p>Install the BAML wrapper with:</p>
        <Code>brew install boundaryml/tap/baml</Code>

        <details className="qs-details">
          <summary>Linux?</summary>
          <Code>curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s</Code>
        </details>

        <details className="qs-details">
          <summary>Arch Linux?</summary>
          <Code>{`yay -S baml-bin
# or build from source:
yay -S baml`}</Code>
        </details>

        <details className="qs-details">
          <summary>Windows?</summary>
          <Code>irm https://pkg.boundaryml.com/install.ps1 | iex -Yes</Code>
        </details>

        <p>
          This installs <code>baml</code> to your computer. <code>baml</code> is
          how you use the language and manage your BAML versions. To pick a
          version, choose either <code>nightly</code> or <code>canary</code>{' '}
          (recommended) channel.
        </p>

        <p>Use canary:</p>
        <Code>baml toolchain use canary</Code>

        <p>Use nightly:</p>
        <Code>baml toolchain use nightly</Code>

        <p>
          You can update your BAML version with{' '}
          <code>baml toolchain update</code>. And you can pin a version with{' '}
          <code>use</code> or by using a <code>.toml</code> file.
        </p>

        <h2>Installing Agent Skills</h2>
        <p>You can teach your agent how to use BAML with one command:</p>
        <Code>baml agent install</Code>
        <p>
          It installs or refreshes the latest official BAML agent skills into
          the current project for Claude Code, Codex, and OpenCode.
        </p>

        <h2>Installing Code/Cursor Extensions</h2>
        <p>
          BAML has a great DX. You can see and test your code through our VSCode
          / Cursor extension, and installing it couldn&rsquo;t be simpler:
        </p>
        <p>Cursor:</p>
        <Code>baml ide install --cursor</Code>
        <p>VS Code:</p>
        <Code>baml ide install --vscode</Code>

        <h2>Useful Links to Keep Handy</h2>
        <ul className="qs-links">
          <li>
            Changelog:{' '}
            <a href="https://new.boundaryml.com/changelog">
              https://new.boundaryml.com/changelog
            </a>
          </li>
          <li>
            Demo repo:{' '}
            <a
              href="https://github.com/boundaryml/baml-demos"
              rel="noopener noreferrer"
              target="_blank"
            >
              https://github.com/boundaryml/baml-demos
            </a>
          </li>
          <li>
            Docs: no docs! use <code>baml describe</code> !
          </li>
          <li>
            BAML skill / agent setup: <code>baml agent install</code>
          </li>
          <li>
            Discord:{' '}
            <a
              href="https://www.boundaryml.com/discord"
              rel="noopener noreferrer"
              target="_blank"
            >
              https://www.boundaryml.com/discord
            </a>
          </li>
          <li>
            Early access onboarding:{' '}
            <a href="https://new.boundaryml.com/eap">
              https://new.boundaryml.com/eap
            </a>
          </li>
        </ul>

        <h2>Footnotes</h2>
        <p>
          The BAML wrapper needs few updates since it is only a version manager.
          You can update it via your package manager, say brew, with{' '}
          <code>brew upgrade baml</code>. If you did not use a package manager to
          install <code>baml</code>, you can update the wrapper with{' '}
          <code>baml self-update</code>. Of course, this information and
          everything above is discoverable via the binary itself; agents
          shouldn&rsquo;t have any trouble, and if they do, let us know!
        </p>
      </main>
      <FooterSection />
    </>
  );
}
