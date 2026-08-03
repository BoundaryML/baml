import type { Metadata } from "next";
import { Inter, JetBrains_Mono, Source_Serif_4 } from "next/font/google";
import { Navbar } from "@/components/navbar";
import { NavRail, NavStrip } from "./_components/nav";
import { Providers } from "./_components/providers";

const inter = Inter({
  subsets: ["latin"],
  variable: "--font-atb-inter",
});

const sourceSerif = Source_Serif_4({
  subsets: ["latin"],
  variable: "--font-atb-ss4",
});

const jetbrains = JetBrains_Mono({
  subsets: ["latin"],
  variable: "--font-atb-jb",
});

export const metadata: Metadata = {
  description:
    "Live dashboard for the BAML benchmark loop: runs, transcripts, agents, and issues.",
  title: "agent tries baml",
};

export default function AtbLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <div
      className={`atb-root min-h-screen flex flex-col ${inter.variable} ${sourceSerif.variable} ${jetbrains.variable}`}
    >
      <Providers>
        <Navbar />
        <div className="flex-1 w-full max-w-6xl mx-auto px-5 sm:px-8 pb-24 md:flex md:gap-10">
          {/* vertical section rail, pinned under the fixed site navbar */}
          <aside className="hidden md:block w-36 shrink-0">
            <div className="sticky top-[130px] pt-10">
              <NavRail />
            </div>
          </aside>
          {/* small screens get a horizontal strip instead */}
          <div className="md:hidden pt-6">
            <NavStrip />
          </div>
          <main className="flex-1 min-w-0">{children}</main>
        </div>
        <footer className="border-t border-atb-line py-8">
          <div className="max-w-6xl mx-auto px-5 sm:px-8 text-xs text-atb-ink-3 flex items-center justify-between">
            <span className="font-atb-serif">agent tries baml</span>
          </div>
        </footer>
      </Providers>
    </div>
  );
}
