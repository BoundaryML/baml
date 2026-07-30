import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import Script from "next/script";
import "./globals.css";
import { ConvexClientProvider } from "@/components/providers/convex-provider";
import { UserProvider } from "@/components/providers/user-provider";
import { ThemeToggle } from "@/components/ui/theme-toggle";
import { THEME_STORAGE_KEY } from "@/lib/theme";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "BEP Feedback",
  description: "BAML Enhancement Proposals Feedback Application",
};

const themeInitScript = `
// NOTE: Keep in sync with theme logic in theme-toggle.tsx.
(() => {
  try {
    const STORAGE_KEY = "${THEME_STORAGE_KEY}";
    const stored = window.localStorage.getItem(STORAGE_KEY);
    const theme =
      stored === "light" || stored === "dark" || stored === "system"
        ? stored
        : "system";
    const resolved =
      theme === "system"
        ? window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light"
        : theme;

    const root = document.documentElement;
    root.classList.remove("light", "dark");
    root.classList.add(resolved);
    root.style.colorScheme = resolved;
  } catch {}
})();
`;

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body
        className={`${geistSans.variable} ${geistMono.variable} antialiased`}
      >
        <Script id="theme-init" strategy="beforeInteractive">
          {themeInitScript}
        </Script>
        <ConvexClientProvider>
          <UserProvider>
            {children}
            <ThemeToggle />
          </UserProvider>
        </ConvexClientProvider>
      </body>
    </html>
  );
}
