import type { Metadata } from 'next';
import { Geist, Geist_Mono } from 'next/font/google';
import Link from 'next/link';
import Script from 'next/script';
import { currentUser } from '@/lib/auth';
import './globals.css';
import { ThemeToggle } from '@/components/ui/theme-toggle';
import { THEME_STORAGE_KEY } from '@/lib/theme';

const geistSans = Geist({ subsets: ['latin'], variable: '--font-geist-sans' });
const geistMono = Geist_Mono({
  subsets: ['latin'],
  variable: '--font-geist-mono',
});

export const metadata: Metadata = {
  description:
    'Issues from user feedback and how far the pipeline has taken each one',
  title: 'BAML Feedback',
};

const themeInitScript = `
(() => {
  try {
    const stored = window.localStorage.getItem("${THEME_STORAGE_KEY}");
    const theme = stored === "light" || stored === "dark" || stored === "system" ? stored : "system";
    const resolved = theme === "system"
      ? (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light")
      : theme;
    const root = document.documentElement;
    root.classList.remove("light", "dark");
    root.classList.add(resolved);
    root.style.colorScheme = resolved;
  } catch {}
})();
`;

export default async function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  const user = await currentUser();
  return (
    <html lang="en" suppressHydrationWarning>
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased`}
      >
        <Script id="theme-init" strategy="beforeInteractive">
          {themeInitScript}
        </Script>
        <header className="border-b">
          <div className="max-w-[1400px] mx-auto px-4 h-14 flex items-center justify-between">
            <Link className="flex items-center gap-3" href="/">
              <span className="font-mono text-xs px-1.5 py-0.5 rounded border bg-muted">
                atb2
              </span>
              <span className="font-semibold">BAML Feedback</span>
            </Link>
            <nav className="flex items-center gap-5 text-sm text-muted-foreground">
              <Link className="text-foreground" href="/">
                Issues
              </Link>
              <Link href="/feedback">Reports</Link>
              <Link href="/intuition">Intuition</Link>
              <Link href="/runs">Runs</Link>
              <Link href="/agents">Agents</Link>
              {user ? (
                <span className="text-foreground">@{user}</span>
              ) : (
                <a href="/api/auth/github">Sign in</a>
              )}
            </nav>
          </div>
        </header>
        {children}
        <ThemeToggle />
      </body>
    </html>
  );
}
