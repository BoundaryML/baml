import Link from 'next/link';

const NAV_LINKS: { label: string; href: string }[] = [
  { label: 'Home', href: '/' },
  { label: 'Install', href: '/learn' },
  { label: 'Learning', href: '/learn' },
  { label: 'Docs', href: 'https://docs.boundaryml.com' },
  { label: 'Guides', href: '/blog' },
  { label: 'Blog', href: '/blog' },
];

export function MinimalNav() {
  return (
    <nav
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '24px 48px',
        borderBottom: '1px solid #E8E3D7',
        background: '#FBFAF5',
        fontFamily: 'var(--font-sans), system-ui, sans-serif',
      }}
    >
      <Link
        href="/"
        style={{
          fontSize: 14,
          fontWeight: 600,
          letterSpacing: '0.18em',
          color: '#1A1612',
          textDecoration: 'none',
        }}
      >
        BAML
      </Link>
      <ul
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 36,
          margin: 0,
          padding: 0,
          listStyle: 'none',
        }}
      >
        {NAV_LINKS.map((link) => (
          <li key={link.label}>
            <Link
              href={link.href}
              style={{
                fontSize: 12,
                fontWeight: 500,
                letterSpacing: '0.16em',
                textTransform: 'uppercase',
                color: '#3F3B36',
                textDecoration: 'none',
              }}
            >
              {link.label}
            </Link>
          </li>
        ))}
      </ul>
    </nav>
  );
}
