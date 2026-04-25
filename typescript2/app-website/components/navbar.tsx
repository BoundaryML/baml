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
      <div style={navStyles.navDiv}>
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
          rel="noopener noreferrer"
          className="mr-6 text-[13px] tracking-[0.15em] uppercase text-muted-foreground hover:text-[#6D28D9] transition-colors"
        >
          Docs
        </Link>
      </div>
      <NavStars />
      <AgentModeToggle />
      <Link
        href="https://docs.boundaryml.com/home"
        target="_blank"
        rel="noopener noreferrer"
        style={{
          ...navStyles.navItem,
          color: '#e8d5ff',
          backgroundColor: '#6d28d9',
          border: '2px solid #a78bfa',
          borderRadius: '8px',
          padding: '6px 14px',
        }}
        className="hover:opacity-90 transition-opacity"
      >
        Learn BAML
      </Link>
    </nav>
  );
}
