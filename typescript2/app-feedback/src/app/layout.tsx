import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import Link from "next/link";
import Script from "next/script";
import "./globals.css";
import { ThemeToggle } from "@/components/ui/theme-toggle";
import { THEME_STORAGE_KEY } from "@/lib/theme";

const geistSans = Geist({ variable: "--font-geist-sans", subsets: ["latin"] });
const geistMono = Geist_Mono({ variable: "--font-geist-mono", subsets: ["latin"] });

export const metadata: Metadata = {
  title: "BAML Feedback",
  description: "Issues from user feedback and how far the pipeline has taken each one",
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

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className={`${geistSans.variable} ${geistMono.variable} antialiased`}>
        <Script id="theme-init" strategy="beforeInteractive">
          {themeInitScript}
        </Script>
        <header className="border-b">
          <div className="max-w-[1400px] mx-auto px-4 h-14 flex items-center justify-between">
            <Link href="/" className="flex items-center gap-3">
              <span className="font-mono text-xs px-1.5 py-0.5 rounded border bg-muted">atb2</span>
              <span className="font-semibold">BAML Feedback</span>
            </Link>
            <nav className="flex items-center gap-5 text-sm text-muted-foreground">
              <Link href="/" className="text-foreground">
                Issues
              </Link>
              <span className="cursor-default">Feedback</span>
              <span className="cursor-default">Evals</span>
              <span className="text-xs border rounded px-1.5 py-0.5">mock</span>
            </nav>
          </div>
        </header>
        {children}
        <ThemeToggle />
      </body>
    </html>
  );
}
