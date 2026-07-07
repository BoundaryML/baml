'use client';

import Image from 'next/image';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { siteConfig } from '@/app/_lib/config';

type GitHubRepoResponse = {
  stargazers_count?: number;
};

const navStyles = {
  logo: {
    fontWeight: 600,
    padding: '0 16px',
    paddingLeft: 0,
  } as React.CSSProperties,
  nav: {
    alignItems: 'center',
    backdropFilter: 'blur(14px)',
    backgroundColor: 'rgba(251, 247, 237, 0.92)',
    borderBottom: '1px solid #D9D3C4',
    boxSizing: 'border-box',
    columnGap: '16px',
    display: 'grid',
    fontFamily: 'var(--font-geist-mono), ui-monospace, "SF Mono", monospace',
    fontSize: '14px',
    gridTemplateColumns: 'auto minmax(0, 1fr) auto',
    left: 0,
    letterSpacing: '0.02em',
    padding: '8px 24px',
    position: 'fixed',
    right: 0,
    textTransform: 'none',
    top: 41,
    width: '100%',
    zIndex: 50,
  } as React.CSSProperties,
  navDiv: {
    minWidth: 0,
    padding: '0 16px',
  } as React.CSSProperties,
  navItem: {
    padding: '0 16px',
    textAlign: 'right' as const,
  } as React.CSSProperties,
  navSpacer: {
    height: 84,
  } as React.CSSProperties,
};

// Fallback star count shown before the live GitHub fetch resolves (or if it
// fails), so the nav always renders a real number instead of a spinner/blank.
const FALLBACK_STARS = 8423;

function NavStars() {
  const [stars, setStars] = useState<number>(FALLBACK_STARS);
  const [hovered, setHovered] = useState(false);

  useEffect(() => {
    fetch('https://api.github.com/repos/boundaryml/baml')
      .then((r) => r.json())
      .then((d: GitHubRepoResponse) => {
        if (typeof d.stargazers_count === 'number') {
          setStars(d.stargazers_count);
        }
      })
      .catch(() => {});
  }, []);

  // Pin the locale: a bare toLocaleString() uses the viewer's locale on the
  // client but en-US on the server, so a non-US digit separator (e.g. "8.423")
  // hydration-mismatches against the server's "8,423".
  const display = (hovered ? stars + 1 : stars).toLocaleString('en-US');

  return (
    <Link
      aria-label={`BAML on GitHub, ${stars.toLocaleString('en-US')} stars`}
      className="flex items-center gap-1.5 hover:text-[#6D28D9] transition-colors"
      href="https://github.com/boundaryml/baml"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      rel="noopener noreferrer"
      style={navStyles.navItem}
      target="_blank"
    >
      <Image
        alt="GitHub"
        className="size-3.5 transition-all duration-150"
        height={14}
        src="/github-mark.svg"
        style={{
          filter: hovered
            ? 'invert(27%) sepia(80%) saturate(800%) hue-rotate(240deg) brightness(90%)'
            : 'none',
          opacity: hovered ? 1 : 0.6,
        }}
        width={14}
      />
      <span className="min-w-[4ch] tabular-nums">{display}</span>
    </Link>
  );
}

function NavDiscord() {
  return (
    <Link
      aria-label="Join the BAML Discord"
      className="nav-social"
      href="https://boundaryml.com/discord"
      rel="noopener noreferrer"
      target="_blank"
    >
      <Image alt="Discord" height={16} src="/discord-icon.svg" width={16} />
    </Link>
  );
}

export function Navbar() {
  const [open, setOpen] = useState(false);

  return (
    <>
      <nav className="nav-responsive" style={navStyles.nav}>
        <Link href="/" onClick={() => setOpen(false)} style={navStyles.logo}>
          BAML
        </Link>
        <div className="nav-links" style={navStyles.navDiv}>
          {siteConfig.nav.links.map((link) => (
            <Link className="nav-link" href={link.href} key={link.id}>
              {link.name}
            </Link>
          ))}
          <Link className="nav-link" href="/changelog">
            Changelog
          </Link>
          <Link className="nav-link" href="/atb">
            agent tries baml
          </Link>
        </div>
        <div className="nav-desktop-actions">
          <NavDiscord />
          <NavStars />
          <Link className="nav-cta" href="/explore">
            <Image
              alt=""
              aria-hidden
              className="nav-cta-lamb"
              height={16}
              src="/baml-lamb-white.png"
              width={16}
            />
            Learn BAML
          </Link>
        </div>

        <button
          aria-expanded={open}
          aria-label={open ? 'Close menu' : 'Open menu'}
          className="nav-toggle"
          onClick={() => setOpen((v) => !v)}
          type="button"
        >
          <span className={`nav-toggle-bar${open ? ' is-open-1' : ''}`} />
          <span className={`nav-toggle-bar${open ? ' is-open-2' : ''}`} />
          <span className={`nav-toggle-bar${open ? ' is-open-3' : ''}`} />
        </button>

        <div className={`nav-mobile-panel${open ? ' is-open' : ''}`}>
          <Link
            className="nav-cta nav-cta--mobile"
            href="/explore"
            onClick={() => setOpen(false)}
          >
            <Image
              alt=""
              aria-hidden
              className="nav-cta-lamb"
              height={18}
              src="/baml-lamb-white.png"
              width={18}
            />
            Learn BAML
          </Link>
          {siteConfig.nav.links.map((link) => (
            <Link
              className="nav-mobile-link"
              href={link.href}
              key={link.id}
              onClick={() => setOpen(false)}
            >
              {link.name}
            </Link>
          ))}
          <Link
            className="nav-mobile-link"
            href="/changelog"
            onClick={() => setOpen(false)}
          >
            Changelog
          </Link>
          <Link
            className="nav-mobile-link"
            href="/atb"
            onClick={() => setOpen(false)}
            rel="noopener noreferrer"
            target="_blank"
          >
            agent tries baml
          </Link>
          <div className="nav-mobile-footer">
            <NavDiscord />
            <NavStars />
          </div>
        </div>

        <style>{`
          .nav-link {
            display: inline-flex;
            align-items: center;
            margin-right: 14px;
            padding: 5px 10px;
            border-radius: 8px;
            font-size: 13.5px;
            letter-spacing: 0.01em;
            text-transform: lowercase;
            color: #5C5852;
            background-color: transparent;
            transition: background-color 140ms ease, color 140ms ease;
          }
          .nav-link:hover {
            background-color: #F0ECE0;
            color: #1A1612;
          }
          /* "Learn BAML" — a filled purple CTA that mirrors the homepage
             "Explore BAML" button so it stands out in the nav. */
          .nav-cta {
            display: inline-flex;
            align-items: center;
            gap: 7px;
            margin-left: 6px;
            padding: 7px 15px;
            border-radius: 9px;
            font-size: 13.5px;
            font-weight: 600;
            letter-spacing: 0.01em;
            text-transform: none;
            white-space: nowrap;
            color: #fff;
            background-color: #6D28D9;
            border: 1px solid #6D28D9;
            box-shadow: 0 1px 2px rgba(109, 40, 217, 0.25);
            transition: background-color 140ms ease, border-color 140ms ease,
              transform 140ms ease, box-shadow 140ms ease;
          }
          .nav-cta:hover {
            background-color: #5B21B6;
            border-color: #5B21B6;
            transform: translateY(-1px);
            box-shadow: 0 3px 10px rgba(109, 40, 217, 0.35);
          }
          /* the lamb asset is purple; brighten it to white for the filled CTA */
          .nav-cta-lamb {
            filter: brightness(0) invert(1);
          }
          .nav-cta--mobile {
            justify-content: center;
            margin: 8px 6px 4px;
            padding: 13px 16px;
            font-size: 15px;
            border-radius: 10px;
          }
          .nav-desktop-actions {
            display: flex;
            align-items: center;
            gap: 4px;
          }
          .nav-social {
            display: inline-flex;
            align-items: center;
            padding: 5px 10px;
          }
          .nav-social img {
            opacity: 0.6;
            transition: opacity 140ms ease, filter 140ms ease;
          }
          .nav-social:hover img {
            opacity: 1;
            filter: invert(27%) sepia(80%) saturate(800%) hue-rotate(240deg) brightness(90%);
          }
          .nav-toggle { display: none; }
          .nav-mobile-panel { display: none; }

          @media (max-width: 860px) {
            .nav-responsive {
              grid-template-columns: 1fr auto !important;
            }
            .nav-links { display: none !important; }
            .nav-desktop-actions { display: none !important; }
            .nav-toggle {
              display: inline-flex;
              flex-direction: column;
              justify-content: center;
              gap: 5px;
              width: 40px;
              height: 40px;
              padding: 8px 9px;
              border: 1px solid #D9D3C4;
              border-radius: 8px;
              background: transparent;
              cursor: pointer;
            }
            .nav-toggle-bar {
              display: block;
              height: 1.5px;
              width: 100%;
              background: #1A1612;
              border-radius: 2px;
              transition: transform 180ms ease, opacity 140ms ease;
            }
            .nav-toggle-bar.is-open-1 { transform: translateY(6.5px) rotate(45deg); }
            .nav-toggle-bar.is-open-2 { opacity: 0; }
            .nav-toggle-bar.is-open-3 { transform: translateY(-6.5px) rotate(-45deg); }
            .nav-mobile-panel {
              display: flex;
              grid-column: 1 / -1;
              flex-direction: column;
              max-height: 0;
              overflow: hidden;
              opacity: 0;
              transition: max-height 240ms ease, opacity 200ms ease, margin-top 240ms ease;
              margin-top: 0;
            }
            .nav-mobile-panel.is-open {
              max-height: 70vh;
              overflow-y: auto;
              opacity: 1;
              margin-top: 12px;
            }
            .nav-mobile-link {
              padding: 14px 6px;
              border-top: 1px solid #E7E1D3;
              font-size: 15px;
              letter-spacing: 0.01em;
              text-transform: lowercase;
              color: #1A1612;
            }
            .nav-mobile-footer {
              display: flex;
              align-items: center;
              justify-content: space-between;
              gap: 16px;
              padding: 16px 6px 4px;
              border-top: 1px solid #E7E1D3;
              text-transform: none;
              letter-spacing: normal;
            }
          }
        `}</style>
      </nav>
      <div aria-hidden className="nav-spacer" style={navStyles.navSpacer} />
    </>
  );
}
