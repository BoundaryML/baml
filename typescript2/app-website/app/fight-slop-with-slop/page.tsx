import type { Metadata } from "next";
import { Navbar } from "@/components/navbar";
import { FooterSection } from "@/components/footer-section";
import WarScene from "./_components/WarScene";
import WarEpilogue from "./_components/WarEpilogue";
import PledgeSection from "./_components/PledgeSection";
import "./war-on-slop.css";

export const metadata: Metadata = {
  title: "War on Slop",
  description:
    "A standing legion against the great unwashed tide of AI slop. Order, craft, and the hand-made; fighting slop with slop.",
  openGraph: {
    title: "The War on Slop",
    description:
      "Order, craft, and the hand-made versus the great unwashed tide.",
    siteName: "BAML",
    type: "website",
  },
};

export default function Page() {
  return (
    <>
      <Navbar />
      <main className="war-on-slop w-full overflow-x-clip">
        <section className="relative z-10 px-6 pt-20 text-center sm:pt-28">
          <h1 className="text-4xl font-bold tracking-tight text-wos-ink sm:text-6xl">
            fight slop with slop
          </h1>
          <p className="tweet-font mt-3 text-sm text-wos-accent">
            by{" "}
            <a
              href="https://x.com/boundaryml"
              target="_blank"
              rel="noopener noreferrer"
              className="hover:underline"
            >
              @boundaryml
            </a>
          </p>
        </section>

        <WarScene />

        <WarEpilogue />

        <PledgeSection />
      </main>
      <FooterSection />
    </>
  );
}
