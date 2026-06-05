import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "agent-tries-baml",
  description:
    "Cross-language perf and Claude Code agent metrics for BAML, captured on the Fly cloud worker.",
};

/**
 * Root server-component layout wrapping every page in the html/body shell.
 * The dashboard stylesheet is scoped under `.atb-scope`, so the body wraps
 * children in that class (and sets the page background) for the styles to apply.
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
      <body style={{ background: "#FBF7ED" }}>
        <div className="atb-scope">{children}</div>
      </body>
    </html>
  );
}
