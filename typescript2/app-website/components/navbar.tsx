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
    columnGap: '16px',
    display: 'grid',
    fontSize: '15px',
    gridTemplateColumns: 'auto 1fr auto auto auto',
    letterSpacing: '0.05em',
    padding: '16px 24px',
    position: 'fixed',
    textTransform: 'uppercase',
    top: 0,
    width: '100%',
    zIndex: 50,
  } as React.CSSProperties,
  navDiv: {
    padding: '0 16px',
  } as React.CSSProperties,
  navItem: {
    padding: '0 16px',
    textAlign: 'right' as const,
  } as React.CSSProperties,
  navSpacer: {
    height: 65,
  } as React.CSSProperties,
};

function NavStars() {
  const [stars, setStars] = useState<number | undefined>(undefined);
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

  const display =
    stars !== undefined
      ? (hovered ? stars + 1 : stars).toLocaleString()
      : '...';

  return (
    <Link
      aria-label={
        stars !== undefined
          ? `BAML on GitHub, ${stars.toLocaleString()} stars`
          : 'BAML on GitHub, loading star count'
      }
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
      <span
        aria-hidden
        style={{
          color: hovered ? '#6D28D9' : '#8A8178',
          fontSize: 11,
          lineHeight: 1,
        }}
      >
        ★
      </span>
      <span className="min-w-[4ch] tabular-nums">{display}</span>
    </Link>
  );
}

export function Navbar() {
  return (
    <>
      <nav className="nav-responsive" style={navStyles.nav}>
        <Link href="/" style={navStyles.logo}>
          Boundary
        </Link>
        <div className="nav-links" style={navStyles.navDiv}>
          {siteConfig.nav.links.map((link) => (
            <Link className="nav-link" href={link.href} key={link.id}>
              {link.name}
            </Link>
          ))}
          <Link
            className="nav-link"
            href="https://docs.boundaryml.com/?utm_source=marketing-site&utm_medium=navbar-docs"
            rel="noopener noreferrer"
            target="_blank"
          >
            Docs
          </Link>
          <Link className="nav-link" href="/vs">
            BAML vs X
          </Link>
        </div>
        <NavStars />
        <ForAgentsLink />
        <LearnBamlLink />
        <style>{`
          .nav-link {
            display: inline-flex;
            align-items: center;
            margin-right: 16px;
            padding: 6px 12px;
            border-radius: 8px;
            font-size: 13px;
            letter-spacing: 0.15em;
            text-transform: uppercase;
            color: #5C5852;
            background-color: transparent;
            transition: background-color 140ms ease, color 140ms ease;
          }
          .nav-link:hover {
            background-color: #F0ECE0;
            color: #1A1612;
          }
        `}</style>
      </nav>
      <div aria-hidden style={navStyles.navSpacer} />
    </>
  );
}

function ForAgentsLink() {
  const [hovered, setHovered] = useState(false);

  return (
    <Link
      href="/llms.txt"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        ...navStyles.navItem,
        border: `1px solid ${hovered ? '#A8A29E' : '#D9D3C4'}`,
        borderRadius: 8,
        color: hovered ? '#1A1612' : '#5C5852',
        padding: '6px 14px',
        transition: 'color 180ms ease, border-color 180ms ease',
      }}
    >
      For agents
    </Link>
  );
}

function LearnBamlLink() {
  const [hovered, setHovered] = useState(false);

  return (
    <Link
      href="https://docs.boundaryml.com/home"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      rel="noopener noreferrer"
      style={{
        ...navStyles.navItem,
        background: hovered
          ? 'linear-gradient(135deg, #DDD0F7 0%, #C4B5FD 100%)'
          : 'linear-gradient(135deg, #F5EFFE 0%, #E9DDFB 100%)',
        border: `1px solid ${hovered ? '#A78BFA' : '#D8C8F5'}`,
        borderRadius: 8,
        color: hovered ? '#4C1D95' : '#6D28D9',
        padding: '6px 14px',
        transition:
          'background 850ms ease, color 850ms ease, border-color 850ms ease',
      }}
      target="_blank"
    >
      Learn BAML
    </Link>
  );
}
