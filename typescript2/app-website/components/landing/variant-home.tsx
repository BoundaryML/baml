'use client';

import Link from 'next/link';
import React, { useState, useEffect } from 'react';
import dynamic from 'next/dynamic';

const BamlPlayground = dynamic(
  () => import('@/playground/BamlPlayground').then((m) => m.BamlPlayground),
  {
    ssr: false,
    loading: () => (
      <div className="flex items-center justify-center min-h-[400px] text-muted-foreground text-sm">
        Loading playground...
      </div>
    ),
  },
);
import { ScriptCopyBtn } from '../magicui/script-copy-btn';
import Marquee from '../magicui/marquee';
import { siteConfig } from '@/app/_lib/config';

const TRUST_LOGOS = [
  { alt: 'SAP', src: '/testimonials/logos/sapLogo.png' },
  { alt: 'AWS', src: '/testimonials/logos/aws.png' },
  { alt: 'AMD', src: '/testimonials/logos/amd.png' },
  { alt: 'Cisco', src: '/testimonials/logos/cisco.png' },
  { alt: 'EY', src: '/testimonials/logos/ey.png' },
  { alt: 'Product Hunt', src: '/testimonials/logos/product-hunt.png' },
  { alt: 'Aer Compliance', src: '/testimonials/logos/aer.png' },
  { alt: 'PMMI', src: '/testimonials/logos/pmmi.png' },
  { alt: 'Cerebral Valley', src: '/testimonials/logos/cerebral.png' },
];

const TrustMarquee = () => (
  <div style={{ marginTop: '56px', width: '100%' }}>
    <p style={{ fontSize: '11px', fontWeight: 500, letterSpacing: '0.08em', textTransform: 'uppercase', color: '#8A8580', marginBottom: '12px' }}>
      Trusted by developers at
    </p>
    <div
      className="relative w-full overflow-hidden"
      style={{
        maskImage: 'linear-gradient(to right, transparent 0%, #000 10%, #000 90%, transparent 100%)',
        WebkitMaskImage: 'linear-gradient(to right, transparent 0%, #000 10%, #000 90%, transparent 100%)',
      }}
    >
      <Marquee className="[--duration:40s] [--gap:3rem] py-2">
        {TRUST_LOGOS.map((logo) => (
          <div key={logo.alt} className="relative h-14 w-36 flex-shrink-0 flex items-center justify-center">
            <img
              src={logo.src}
              alt={logo.alt}
              className="max-h-full max-w-full object-contain grayscale opacity-60 hover:grayscale-0 hover:opacity-100 transition-all duration-300"
            />
          </div>
        ))}
      </Marquee>
    </div>
  </div>
);

const ROTATING_WORDS = ['writing', 'building', 'running', 'evaluating', 'debugging', 'orchestrating', 'shipping', 'testing', 'training', 'deploying', 'scaling', 'securing', 'streaming'];
const HOLD_MS = 2200;
const TRANSITION_MS = 400;

const RotatingWord = () => {
  const [index, setIndex] = useState(0);
  const [visible, setVisible] = useState(true);
  const [reduced, setReduced] = useState(false);

  useEffect(() => {
    setReduced(window.matchMedia('(prefers-reduced-motion: reduce)').matches);
  }, []);

  useEffect(() => {
    if (reduced) return;

    let interval: ReturnType<typeof setInterval> | null = null;

    const startCycle = () => {
      interval = setInterval(() => {
        setVisible(false);
        setTimeout(() => {
          setIndex((i) => (i + 1) % ROTATING_WORDS.length);
          setVisible(true);
        }, TRANSITION_MS);
      }, HOLD_MS + TRANSITION_MS);
    };

    const handleVisibility = () => {
      if (document.visibilityState === 'hidden') {
        if (interval) clearInterval(interval);
      } else {
        startCycle();
      }
    };

    startCycle();
    document.addEventListener('visibilitychange', handleVisibility);
    return () => {
      if (interval) clearInterval(interval);
      document.removeEventListener('visibilitychange', handleVisibility);
    };
  }, [reduced]);

  return (
    <span
      aria-live="polite"
      aria-atomic="true"
      style={{
        display: 'inline-block',
        fontFamily: 'var(--font-serif)',
        fontStyle: 'italic',
        fontWeight: 500,
        color: '#6D28D9',
        opacity: visible ? 1 : 0,
        transform: visible ? 'translateY(0)' : 'translateY(6px)',
        transition: reduced ? 'none' : `opacity ${TRANSITION_MS}ms cubic-bezier(0.4,0,0.2,1), transform ${TRANSITION_MS}ms cubic-bezier(0.4,0,0.2,1)`,
      }}
    >
      {ROTATING_WORDS[index]}
    </span>
  );
};

const customStyles = {
  root: {
    '--bg': '#F5F1E8',
    '--fg': '#1A1612',
    '--border': '#D9D3C4',
    '--accent': '#6D28D9',
    '--accent-hover': '#5B21B6',
    '--secondary': '#6D28D9',
    '--syn-purple': '#8B5CF6',
    '--syn-green': '#059669',
    '--syn-teal': '#0D9488',
    '--syn-string': '#B45309',
    '--syn-comment': '#78716C',
  } as React.CSSProperties,
  container: {
    width: '100%',
    maxWidth: '1600px',
    margin: '0 auto',
    borderLeft: '1px solid #D9D3C4',
    borderRight: '1px solid #D9D3C4',
    minHeight: '100vh',
    display: 'flex',
    flexDirection: 'column',
    backgroundColor: '#F5F1E8',
    color: '#1A1612',
    fontSize: '14px',
    lineHeight: '1.5',
  } as React.CSSProperties,
  nav: {
    display: 'grid',
    gridTemplateColumns: 'auto 1fr auto auto auto',
    alignItems: 'center',
    columnGap: '16px',
    padding: '16px 24px',
    fontSize: '15px',
    letterSpacing: '0.05em',
    textTransform: 'uppercase',
    borderBottom: '1px solid #D9D3C4',
  } as React.CSSProperties,
  logo: {
    fontWeight: 600,
    padding: '0 16px',
    paddingLeft: 0,
  } as React.CSSProperties,
  navDiv: {
    padding: '0 16px',
  } as React.CSSProperties,
  navItem: {
    padding: '0 16px',
    textAlign: 'right' as const,
  },
  refMark: {
    color: '#6D28D9',
    fontWeight: 'normal' as const,
  },
  hero: {
    display: 'grid',
    gridTemplateColumns: '480px 1fr',
    minHeight: '640px',
    borderBottom: '1px solid #D9D3C4',
  } as React.CSSProperties,
  heroLeft: {
    padding: '48px 48px 64px',
    display: 'flex',
    flexDirection: 'column' as const,
    justifyContent: 'space-between',
    borderRight: '1px solid #D9D3C4',
  },
  heroMeta: {
    fontSize: '12px',
    textTransform: 'uppercase' as const,
    marginBottom: '4rem',
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  heroMetaBottom: {
    fontSize: '12px',
    textTransform: 'uppercase' as const,
    marginTop: '4rem',
    marginBottom: 0,
    display: 'flex',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  h1: {
    fontSize: 'clamp(2rem, 4.5vw, 3.5rem)',
    fontWeight: 600,
    lineHeight: '1.02',
    letterSpacing: '-0.03em',
    color: '#1A1612',
    marginBottom: '1.5rem',
  } as React.CSSProperties,
  h2: {
    fontSize: 'clamp(2rem, 4vw, 3.5rem)',
    fontWeight: 500,
    lineHeight: '1.2',
    letterSpacing: '-0.02em',
  } as React.CSSProperties,
  p: {
    fontSize: '17px',
    fontWeight: 400,
    lineHeight: '1.6',
    color: '#5C5852',
    maxWidth: '480px',
    marginBottom: '1rem',
  },
  ctaContainer: {
    display: 'flex',
    alignItems: 'center',
    marginTop: '2rem',
    gap: '12px',
  },
  ctaLink: {
    display: 'inline-flex',
    alignItems: 'center',
    fontSize: '18px',
    fontWeight: 500,
    textDecoration: 'none',
    color: 'inherit',
    cursor: 'pointer',
    transition: 'color 0.2s ease',
  } as React.CSSProperties,
  ctaLinkSvg: {
    marginLeft: '12px',
    transition: 'transform 0.2s ease',
  },
  botanicalAccent: {
    color: '#6D28D9',
    opacity: 0.6,
  },
  heroRight: {
    padding: 0,
    backgroundColor: 'rgba(255,255,255,0.3)',
    backgroundImage:
      "url(\"data:image/svg+xml,%3Csvg width='40' height='40' viewBox='0 0 40 40' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M20 5c0 0-5 5-5 10s5 10 5 10 5-5 5-10-5-10-5-10zm0 2c2 2 3 6 3 8s-1 6-3 8-3-6-3-8 1-6 3-8z' fill='%237C3AED' fill-opacity='0.05'/%3E%3C/svg%3E\")",
    display: 'flex',
    alignItems: 'stretch',
    justifyContent: 'center',
    overflow: 'hidden',
  } as React.CSSProperties,
  codeWindow: {
    width: '100%',
    backgroundColor: '#FAFAF9',
    border: '1px solid #D9D3C4',
    boxShadow: '0 20px 40px rgba(0,0,0,0.05)',
    borderRadius: '6px',
    overflow: 'hidden',
    display: 'flex',
    flexDirection: 'column' as const,
  },
  codeWindowNoShadow: {
    width: '100%',
    backgroundColor: '#FAFAF9',
    border: '1px solid #D9D3C4',
    boxShadow: 'none',
    borderRadius: '6px',
    overflow: 'hidden',
    display: 'flex',
    flexDirection: 'column' as const,
  },
  codeWindowAccent: {
    width: '100%',
    backgroundColor: '#FAFAF9',
    border: '1px solid #6D28D9',
    boxShadow: 'none',
    borderRadius: '6px',
    overflow: 'hidden',
    display: 'flex',
    flexDirection: 'column' as const,
  },
  windowChrome: {
    display: 'grid',
    gridTemplateColumns: '80px 1fr 80px',
    alignItems: 'center',
    padding: '12px 16px',
    borderBottom: '1px solid #D9D3C4',
    backgroundColor: '#F5F5F4',
  },
  windowDots: {
    display: 'flex',
    gap: '6px',
  },
  dotRed: {
    width: '10px',
    height: '10px',
    borderRadius: '50%',
    border: '1px solid rgba(0,0,0,0.1)',
    backgroundColor: '#FF5F56',
    display: 'inline-block',
  },
  dotYellow: {
    width: '10px',
    height: '10px',
    borderRadius: '50%',
    border: '1px solid rgba(0,0,0,0.1)',
    backgroundColor: '#FFBD2E',
    display: 'inline-block',
  },
  dotGreen: {
    width: '10px',
    height: '10px',
    borderRadius: '50%',
    border: '1px solid rgba(0,0,0,0.1)',
    backgroundColor: '#27C93F',
    display: 'inline-block',
  },
  windowTab: {
    textAlign: 'center' as const,
    fontSize: '12px',
    color: '#57534E',
  },
  codeContentArea: {
    display: 'flex',
    backgroundColor: '#FAFAF9',
  },
  lineNumbers: {
    padding: '16px 12px',
    textAlign: 'right' as const,
    color: '#A8A29E',
    fontSize: '13px',
    userSelect: 'none' as const,
    borderRight: '1px solid #D9D3C4',
    backgroundColor: '#F5F5F4',
    lineHeight: '1.5',
  },
  lineNumbersAccent: {
    padding: '16px 12px',
    textAlign: 'right' as const,
    color: '#6D28D9',
    fontSize: '13px',
    userSelect: 'none' as const,
    borderRight: '1px solid #6D28D9',
    backgroundColor: '#F5F5F4',
    lineHeight: '1.5',
  },
  codeScroll: {
    padding: '16px',
    overflowX: 'auto' as const,
    width: '100%',
  },
  pre: {
    margin: 0,
    fontFamily:
      "'IBM Plex Mono', ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace",
    fontSize: '13px',
    color: '#1A1612',
  },
  statementSection: {
    padding: '48px 48px',
    borderBottom: '1px solid #D9D3C4',
  },
  statementText: {
    maxWidth: '700px',
    margin: '0 auto 48px',
    textAlign: 'center' as const,
  },
  statementP: {
    fontSize: 'clamp(1.1rem, 2vw, 1.6rem)',
    lineHeight: '1.4',
    fontWeight: 300,
    marginBottom: '0.5rem',
  } as React.CSSProperties,
  featureIndex: {
    display: 'flex',
    flexDirection: 'column' as const,
    borderTop: '2px solid #1A1612',
  },
  indexRow: {
    display: 'grid',
    gridTemplateColumns: '60px 1fr 100px',
    padding: '16px 0',
    borderBottom: '1px solid #D9D3C4',
    alignItems: 'baseline',
    transition: 'background-color 0.2s ease',
    cursor: 'default',
  },
  indexNum: {
    fontSize: '12px',
    color: '#78716C',
  },
  indexTitle: {
    fontSize: '18px',
    fontWeight: 500,
  },
  indexMeta: {
    textAlign: 'right' as const,
    fontSize: '12px',
    color: '#6D28D9',
  },
  exhibitHeader: {
    display: 'grid',
    gridTemplateColumns: '1fr auto',
    padding: '40px 48px 20px',
    alignItems: 'baseline',
  },
  exhibitTitle: {
    fontSize: '2rem',
    fontWeight: 500,
  } as React.CSSProperties,
  exhibitGrid: {
    display: 'grid',
    gridTemplateColumns: '1fr 1fr',
    borderTop: '1px solid #D9D3C4',
  },
  exhibitPanel: {
    padding: '48px',
    backgroundColor: 'rgba(255,255,255,0.1)',
  },
  exhibitPanelLeft: {
    padding: '48px',
    backgroundColor: 'rgba(255,255,255,0.1)',
    borderRight: '1px solid #D9D3C4',
  },
  panelHeader: {
    fontSize: '12px',
    textTransform: 'uppercase' as const,
    marginBottom: '24px',
    color: '#6D28D9',
    display: 'flex',
    justifyContent: 'space-between',
  },
  kw: { color: '#8B5CF6', fontWeight: 500 },
  fn: { color: '#0D9488' },
  ty: { color: '#059669' },
  st: { color: '#B45309' },
  cm: { color: '#78716C' },
};

const WindowDots = () => (
  <div style={customStyles.windowDots}>
    <span style={customStyles.dotRed} />
    <span style={customStyles.dotYellow} />
    <span style={customStyles.dotGreen} />
  </div>
);

const NavStars = () => {
  const [stars, setStars] = useState<number | undefined>(undefined);
  const [hovered, setHovered] = useState(false);
  useEffect(() => {
    fetch('https://api.github.com/repos/boundaryml/baml')
      .then((r) => r.json())
      .then((d) => setStars(d.stargazers_count as number))
      .catch(() => { });
  }, []);

  const display = stars !== undefined
    ? (hovered ? stars + 1 : stars).toLocaleString()
    : 'GitHub';

  return (
    <Link
      href="https://github.com/boundaryml/baml"
      target="_blank"
      style={customStyles.navItem}
      className="flex items-center gap-1.5 hover:text-[#6D28D9] transition-colors"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <img
        src="/github-mark.svg"
        alt="GitHub"
        className="size-3.5 transition-all duration-150"
        style={{ opacity: hovered ? 1 : 0.6, filter: hovered ? 'invert(27%) sepia(80%) saturate(800%) hue-rotate(240deg) brightness(90%)' : 'none' }}
      />
      <span className="tabular-nums">{display}</span><span style={customStyles.refMark}>)</span>
    </Link>
  );
};

const Nav = () => (
  <nav
    className="nav-responsive"
    style={customStyles.nav}
  >
    <div style={customStyles.logo}>
      Boundary
    </div>
    <div style={customStyles.navDiv}>
      {siteConfig.nav.links.map((link) => (
        <Link
          key={link.id}
          href={link.href}
          className="mr-6 text-[13px] tracking-[0.15em] uppercase text-muted-foreground hover:text-[#6D28D9] transition-colors"
        >
          {link.name}
        </Link>
      ))}
      <Link
        href="https://docs.boundaryml.com/?utm_source=marketing-site&utm_medium=navbar-docs"
        target="_blank"
        className="mr-6 text-[13px] tracking-[0.15em] uppercase text-muted-foreground hover:text-[#6D28D9] transition-colors"
      >
        Docs
      </Link>
    </div>
    <NavStars />
    <Link
      href="/signin"
      style={{ ...customStyles.navItem, padding: '6px 10px', borderRadius: '8px' }}
      className="hover:bg-[#e0dbd3] transition-colors"
    >
      Sign in
    </Link>
    <Link
      href="/try"
      style={{ ...customStyles.navItem, color: '#e8d5ff', backgroundColor: '#6d28d9', border: '2px solid #a78bfa', borderRadius: '8px', padding: '6px 14px' }}
      className="hover:opacity-90 transition-opacity"
    >
      Start your project
    </Link>
  </nav>
);

const HeroCodeWindow = () => (
  <div style={customStyles.codeWindow}>
    <div style={customStyles.windowChrome}>
      <WindowDots />
      <div style={customStyles.windowTab}>extract_receipt.baml</div>
      <div />
    </div>
    <div style={customStyles.codeContentArea}>
      <div style={customStyles.lineNumbers}>
        {[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((n) => (
          <div key={n}>{n}</div>
        ))}
      </div>
      <div style={customStyles.codeScroll}>
        <pre style={customStyles.pre}>
          <code>
            <span style={customStyles.kw}>class</span>{' '}
            <span style={customStyles.ty}>Receipt</span> {'{'}
            {'\n'} total <span style={customStyles.ty}>float</span> @description(
            <span style={customStyles.st}>"Final amount paid"</span>)
            {'\n'} items <span style={customStyles.ty}>Item[]</span>
            {'\n'} date <span style={customStyles.ty}>string</span> @description(
            <span style={customStyles.st}>"YYYY-MM-DD"</span>)
            {'\n'}
            {'}'}
            {'\n'}
            {'\n'}
            <span style={customStyles.kw}>function</span>{' '}
            <span style={customStyles.fn}>ExtractReceipt</span>(img:{' '}
            <span style={customStyles.ty}>Image</span>) -&gt;{' '}
            <span style={customStyles.ty}>Receipt</span> {'{'}
            {'\n'} <span style={customStyles.kw}>client</span> GPT4o
            {'\n'} <span style={customStyles.kw}>prompt</span> #"
            {'\n'} Extract the items and total from this receipt.
            {'\n'} {'{{ ctx.output_format }}'}
            {'\n'}
            {'\n'} Receipt: {'{{ img }}'}
            {'\n'} "#
            {'\n'}
            {'}'}
          </code>
        </pre>
      </div>
    </div>
  </div>
);

type InstallPath = 'claude' | 'codex' | 'human';

const humanCommand = 'curl -fsSL https://baml.dev/install | sh';

const installOptions: { id: InstallPath; label: string; icon?: string; command: string }[] = [
  { id: 'claude', label: 'Claude', icon: '/Claude Color SVG.svg', command: 'curl -fsSL http://baml.dev/agent | claude' },
  { id: 'codex', label: 'Codex', icon: '/Codex Color.svg', command: 'curl -fsSL http://baml.dev/agent | codex' },
  { id: 'human', label: 'Human', command: humanCommand },
];

const HeroSection = () => {
  const [installPath, setInstallPath] = useState<InstallPath>('claude');

  const selected = installOptions.find((o) => o.id === installPath)!;

  return (
    <section
      className="hero-responsive"
      style={customStyles.hero}
    >
      <div style={customStyles.heroLeft}>
        <div>
          <h1 style={customStyles.h1}>
            The language
            <br />for <RotatingWord />
            <br />AI.
          </h1>
          <div style={customStyles.ctaContainer}>
            <div className="w-full max-w-xl">
              <p style={{ fontSize: '18px', fontWeight: 500, letterSpacing: '0.08em', textTransform: 'uppercase', color: '#8A8580', marginBottom: '20px' }}>
                Install BAML
              </p>
              <div className="relative">
                {/* invisible spacer always sized to the longest command */}
                <div className="invisible pointer-events-none">
                  <ScriptCopyBtn
                    className="block w-full"
                    codeLanguage="bash"
                    commandMap={{ bash: humanCommand } as const}
                    darkTheme="none"
                    lightTheme="none"
                    showMultiplePackageOptions={false}
                  />
                </div>
                {/* active command overlaid on top */}
                <div className="absolute inset-0">
                  <ScriptCopyBtn
                    className="block w-full"
                    codeLanguage="bash"
                    commandMap={{ bash: selected.command } as const}
                    darkTheme="none"
                    lightTheme="none"
                    showMultiplePackageOptions={false}
                  />
                </div>
              </div>
              <div className="mt-3 flex gap-2">
                {installOptions.map((opt) => (
                  <button
                    key={opt.id}
                    type="button"
                    onClick={() => setInstallPath(opt.id)}
                    className="rounded-md px-3 py-2 text-sm font-medium transition-colors flex items-center gap-1.5"
                    style={installPath === opt.id
                      ? { background: '#1A1612', color: '#fff', border: '1px solid #1A1612' }
                      : { background: 'transparent', color: '#5C5852', border: '1px solid #D9D3C4' }
                    }
                  >
                    {opt.icon && <img src={opt.icon} alt={opt.label} className="size-4" />}
                    {opt.label}
                  </button>
                ))}
              </div>
              <TrustMarquee />
            </div>
          </div>
        </div>
      </div>
      <div
        className="hero-right-responsive relative"
        style={customStyles.heroRight}
      >
        <div className="absolute inset-0 flex flex-col">
          <div className="flex-1 min-h-0">
            <BamlPlayground compact />
          </div>
        </div>
      </div>
    </section>
  );
};

const FeatureIndex = () => {
  const features = [
    { num: '01', title: 'Typescript-inspired type system', meta: 'Core', version: '1.0' },
    { num: '02', title: 'Type-safe non-viral errors', meta: 'Core', version: '1.1' },
    { num: '03', title: 'Semantic grep & parsing', meta: 'Runtime', version: '2.0' },
    { num: '04', title: 'Universal runtime (Python, Rust, JS)', meta: 'System', version: '3.0' },
  ];

  const [hoveredRow, setHoveredRow] = useState<number | null>(null);

  return (
    <div style={customStyles.featureIndex}>
      {features.map((f, i) => (
        <div
          key={f.num}
          className="index-row-responsive"
          style={{
            ...customStyles.indexRow,
            backgroundColor:
              hoveredRow === i ? 'rgba(255,255,255,0.4)' : 'transparent',
          }}
          onMouseEnter={() => setHoveredRow(i)}
          onMouseLeave={() => setHoveredRow(null)}
        >
          <span style={customStyles.indexNum}>{f.num}</span>
          <span style={customStyles.indexTitle}>{f.title}</span>
          <span style={customStyles.indexMeta}>
            {f.meta} <sup>{f.version}</sup>
          </span>
        </div>
      ))}
    </div>
  );
};

const StatementSection = () => (
  <section style={customStyles.statementSection}>
    <div style={customStyles.statementText}>
      <p style={customStyles.statementP}>
        Python and Typescript were built for people.
      </p>
      <p style={customStyles.statementP}>
        BAML is an <strong>agent-optimized</strong> programming language.
      </p>
    </div>
    <FeatureIndex />
  </section>
);

const LegacyCodeWindow = () => (
  <div style={customStyles.codeWindowNoShadow}>
    <div style={customStyles.codeContentArea}>
      <div style={customStyles.lineNumbers}>
        {[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((n) => (
          <div key={n}>{n}</div>
        ))}
      </div>
      <div style={customStyles.codeScroll}>
        <pre style={customStyles.pre}>
          <code>
            <span style={customStyles.kw}>import</span> {'{ OpenAI }'}{' '}
            <span style={customStyles.kw}>from</span>{' '}
            <span style={customStyles.st}>'openai'</span>;
            {'\n'}
            <span style={customStyles.kw}>import</span> {'{ z }'}{' '}
            <span style={customStyles.kw}>from</span>{' '}
            <span style={customStyles.st}>'zod'</span>;
            {'\n'}
            {'\n'}
            <span style={customStyles.kw}>const</span> schema = z.object({'{'}
            {'\n'} intent: z.enum([
            <span style={customStyles.st}>'buy'</span>,{' '}
            <span style={customStyles.st}>'support'</span>]),
            {'\n'} confidence: z.number()
            {'\n'}
            {'}'});
            {'\n'}
            {'\n'}
            <span style={customStyles.kw}>async function</span>{' '}
            <span style={customStyles.fn}>analyze</span>(text:{' '}
            <span style={customStyles.ty}>string</span>) {'{'}
            {'\n'}{' '}
            <span style={customStyles.cm}>
              // Manual prompt construction
            </span>
            {'\n'}{' '}
            <span style={customStyles.cm}>
              // Complex error handling needed
            </span>
            {'\n'}{' '}
            <span style={customStyles.cm}>
              // JSON parsing logic required
            </span>
            {'\n'}{' '}
            <span style={customStyles.cm}>
              // ... 50 lines of boilerplate ...
            </span>
            {'\n'}
            {'}'}
          </code>
        </pre>
      </div>
    </div>
  </div>
);

const BamlCodeWindow = () => (
  <div style={customStyles.codeWindowAccent}>
    <div style={customStyles.codeContentArea}>
      <div style={customStyles.lineNumbersAccent}>
        {[1, 2, 3, 4, 5, 6, 7, 8, 9].map((n) => (
          <div key={n}>{n}</div>
        ))}
      </div>
      <div style={customStyles.codeScroll}>
        <pre style={customStyles.pre}>
          <code>
            <span style={customStyles.kw}>enum</span>{' '}
            <span style={customStyles.ty}>Intent</span> {'{'}
            {'\n'} Buy
            {'\n'} Support
            {'\n'}
            {'}'}
            {'\n'}
            {'\n'}
            <span style={customStyles.kw}>function</span>{' '}
            <span style={customStyles.fn}>Analyze</span>(text:{' '}
            <span style={customStyles.ty}>string</span>) -&gt; (
            <span style={customStyles.ty}>Intent</span>,{' '}
            <span style={customStyles.ty}>float</span>) {'{'}
            {'\n'} <span style={customStyles.kw}>client</span> FastLLM
            {'\n'} <span style={customStyles.kw}>prompt</span> #"
            {'\n'} Analyze this text: {'{{ text }}'}
            {'\n'} {'{{ ctx.output_format }}'}
            {'\n'} "#
            {'\n'}
            {'}'}
          </code>
        </pre>
      </div>
    </div>
  </div>
);

const ExhibitSection = () => (
  <section>
    <div style={customStyles.exhibitHeader}>
      <h2 style={customStyles.exhibitTitle}>Typescript → BAML</h2>
    </div>
    <div
      className="exhibit-grid-responsive"
      style={customStyles.exhibitGrid}
    >
      <div style={customStyles.exhibitPanelLeft}>
        <div style={customStyles.panelHeader}>
          <span>Legacy Implementation</span>
          <span>main.ts</span>
        </div>
        <LegacyCodeWindow />
      </div>
      <div style={customStyles.exhibitPanel}>
        <div style={customStyles.panelHeader}>
          <span>BAML Implementation</span>
          <span>analyze.baml</span>
        </div>
        <BamlCodeWindow />
      </div>
    </div>
  </section>
);

export function VariantHome() {
  useEffect(() => {
    const style = document.createElement('style');
    style.textContent = `
      @media (max-width: 1024px) {
        .hero-responsive { grid-template-columns: 1fr !important; }
        .hero-right-responsive { border-top: 1px solid #D9D3C4; border-right: none !important; }
        .exhibit-grid-responsive { grid-template-columns: 1fr !important; }
      }
      @media (max-width: 600px) {
        .nav-responsive { grid-template-columns: 1fr auto !important; gap: 12px; padding: 16px !important; }
        .index-row-responsive { grid-template-columns: 40px 1fr !important; }
      }
    `;
    document.head.appendChild(style);
    return () => {
      document.head.removeChild(style);
    };
  }, []);

  return (
    <div style={customStyles.root as React.CSSProperties}>
      <div style={customStyles.container}>
        <Nav />
        <HeroSection />
        <StatementSection />
        <ExhibitSection />
      </div>
    </div>
  );
}

