import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "baml-bench",
  description:
    "Cross-language perf and Claude Code agent metrics for BAML, captured on the Fly cloud worker.",
};

/**
 * Root server-component layout wrapping every page in the html/body shell and
 * loading the global stylesheet for the baml-bench dashboard.
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
      <body>{children}</body>
    </html>
  );
}
