'use client';

import Link from 'next/link';
import { useEffect, useState } from 'react';
import { siteConfig } from '@/app/_lib/config';
import { AgentModeToggle } from '@/components/agent-mode-toggle';

const navStyles = {
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
  } as React.CSSProperties,
};

function NavStars() {
  const [stars, setStars] = useState<number | undefined>(undefined);
  const [hovered, setHovered] = useState(false);

  useEffect(() => {
    fetch('https://api.github.com/repos/boundaryml/baml')
      .then((r) => r.json())
      .then((d) => setStars(d.stargazers_count as number))
      .catch(() => {});
  }, []);

  const display =
    stars !== undefined
      ? (hovered ? stars + 1 : stars).toLocaleString()
      : 'GitHub';

  return (
    <Link
      href="https://github.com/boundaryml/baml"
      target="_blank"
      rel="noopener noreferrer"
      style={navStyles.navItem}
      className="flex items-center gap-1.5 hover:text-[#6D28D9] transition-colors"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
    >
      <img
        src="/github-mark.svg"
        alt="GitHub"
        className="size-3.5 transition-all duration-150"
        style={{
          opacity: hovered ? 1 : 0.6,
          filter: hovered
            ? 'invert(27%) sepia(80%) saturate(800%) hue-rotate(240deg) brightness(90%)'
            : 'none',
        }}
      />
      <span className="tabular-nums">{display}</span>
    </Link>
  );
}

export function Navbar() {
  return (
    <nav className="nav-responsive" style={navStyles.nav}>
      <Link href="/" style={navStyles.logo}>
        Boundary
      </Link>
      <div style={navStyles.navDiv} className="nav-links">
        {siteConfig.nav.links.map((link) => (
          <Link
            key={link.id}
            href={link.href}
            className="nav-link"
          >
            {link.name}
          </Link>
        ))}
        <Link
          href="https://docs.boundaryml.com/?utm_source=marketing-site&utm_medium=navbar-docs"
          target="_blank"
          rel="noopener noreferrer"
          className="nav-link"
        >
          Docs
        </Link>
      </div>
      <NavStars />
      <AgentModeToggle />
      <LearnBamlLink />
      <style>{`
        .nav-link {
          display: inline-flex;
          align-items: center;
          margin-right: 4px;
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
  );
}

function LearnBamlLink() {
  const [hovered, setHovered] = useState(false);

  return (
    <Link
      href="https://docs.boundaryml.com/home"
      target="_blank"
      rel="noopener noreferrer"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        ...navStyles.navItem,
        color: hovered ? '#4C1D95' : '#6D28D9',
        background: hovered
          ? 'linear-gradient(135deg, #DDD0F7 0%, #C4B5FD 100%)'
          : 'linear-gradient(135deg, #F5EFFE 0%, #E9DDFB 100%)',
        border: `1px solid ${hovered ? '#A78BFA' : '#D8C8F5'}`,
        borderRadius: 8,
        padding: '6px 14px',
        transition:
          'background 850ms ease, color 850ms ease, border-color 850ms ease',
      }}
    >
      Learn BAML
    </Link>
  );
}
