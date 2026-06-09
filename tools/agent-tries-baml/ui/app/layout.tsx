import type { Metadata } from "next";

import { SiteNav } from "@/components/site-nav";

import "./globals.css";

export const metadata: Metadata = {
  title: "agent-tries-baml",
  description:
    "Cross-language perf and Claude Code agent metrics for BAML, captured on the Fly cloud worker.",
};

/**
 * Root server-component layout wrapping every page in the html/body shell.
 * Page chrome (paper background, body type) comes from the Tailwind base
 * layer; `.atb-page` provides the 760px editorial column every page sits in.
 * @param children - the routed page content rendered inside the body
 * @returns the top-level html document structure
 */
export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body>
        <div className="atb-page">
          <SiteNav />
          {children}
        </div>
      </body>
    </html>
  );
}
