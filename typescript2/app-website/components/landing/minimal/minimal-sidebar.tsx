import Link from 'next/link';

interface SidebarLink {
  label: string;
  href: string;
}

interface MinimalSidebarProps {
  timeline?: string;
  newsTitle?: string;
  newsHref?: string;
  importantLinks?: SidebarLink[];
  communityLinks?: SidebarLink[];
}

const DEFAULT_TIMELINE = 'PYTHON 1991 · JAVA 1995 · TYPESCRIPT 2012 · BAML 2025';

const DEFAULT_IMPORTANT: SidebarLink[] = [
  { label: 'Getting Started', href: '/learn' },
  { label: 'Docs', href: 'https://docs.boundaryml.com' },
  { label: 'GitHub', href: 'https://github.com/BoundaryML/baml' },
];

const DEFAULT_COMMUNITY: SidebarLink[] = [
  { label: 'Discord', href: 'https://discord.gg/BTNBeXGuaS' },
  { label: 'Podcast', href: '/podcast' },
  { label: 'Team', href: '/who-are-we' },
  { label: 'Jobs', href: '/who-are-we' },
];

export function MinimalSidebar({
  timeline = DEFAULT_TIMELINE,
  newsTitle = 'BAML v0.75.0 released',
  newsHref = '/changelog',
  importantLinks = DEFAULT_IMPORTANT,
  communityLinks = DEFAULT_COMMUNITY,
}: MinimalSidebarProps) {
  return (
    <aside
      style={{
        fontFamily: 'var(--font-sans), system-ui, sans-serif',
        color: '#1A1612',
        maxWidth: 320,
      }}
    >
      <p
        style={{
          margin: 0,
          fontFamily:
            '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
          fontSize: 12,
          letterSpacing: '0.08em',
          color: '#6F6A63',
          lineHeight: 1.6,
        }}
      >
        {timeline}
      </p>

      <SidebarSection title="News">
        <Link href={newsHref} style={linkStyle}>
          {newsTitle}
        </Link>
      </SidebarSection>

      <SidebarSection title="Important links">
        <SidebarList links={importantLinks} />
      </SidebarSection>

      <SidebarSection title="Join the community">
        <SidebarList links={communityLinks} />
      </SidebarSection>
    </aside>
  );
}

function SidebarSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section
      style={{
        marginTop: 36,
        paddingTop: 28,
        borderTop: '1px solid #E2DDCF',
      }}
    >
      <h3
        style={{
          margin: 0,
          fontSize: 11,
          fontWeight: 600,
          letterSpacing: '0.16em',
          textTransform: 'uppercase',
          color: '#8A8580',
        }}
      >
        {title}
      </h3>
      <div style={{ marginTop: 16 }}>{children}</div>
    </section>
  );
}

function SidebarList({ links }: { links: SidebarLink[] }) {
  return (
    <ul
      style={{
        margin: 0,
        padding: 0,
        listStyle: 'none',
        display: 'flex',
        flexDirection: 'column',
        gap: 12,
      }}
    >
      {links.map((link) => (
        <li key={link.label}>
          <Link href={link.href} style={linkStyle}>
            {link.label}
          </Link>
        </li>
      ))}
    </ul>
  );
}

const linkStyle: React.CSSProperties = {
  fontSize: 16,
  color: '#1A1612',
  textDecoration: 'none',
  borderBottom: '1px solid transparent',
  transition: 'border-color 120ms ease',
};
