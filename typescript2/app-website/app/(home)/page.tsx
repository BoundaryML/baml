import Adopt from '@/components/home/adopt';
import Agents from '@/components/home/agents';
import Book from '@/components/home/book';
import Footer from '@/components/home/footer';
import ForceDark from '@/components/home/force-dark';
import Hero from '@/components/home/hero';
import Hyperobs, { WhyNow } from '@/components/home/hyperobs';
import Language from '@/components/home/language';
import Logos from '@/components/home/logos';
import Nav from '@/components/home/nav';
import Observe from '@/components/home/observe';
import Practical from '@/components/home/practical';
import Review from '@/components/home/review';
import Roadmap from '@/components/home/roadmap';
import Stack from '@/components/home/stack';
import { CtaClicks, SectionViews } from '@/components/home/track';
import './styles/v2.css';
import './styles/diagram-styles.css';
import './styles/v3.css';
import './styles/home.css';

export default function Home() {
  return (
    <div
      className="v2 v3-page v4-page home-page min-h-screen"
      data-theme="dark"
    >
      <ForceDark />
      <Nav pinBanner={false} />
      <main id="main">
        <Hero />
        <Logos />
        <Hyperobs />
        <WhyNow />
        <Observe />
        <Agents />
        <Practical />
        <Stack />
        <Language />
        <Adopt />
        <Review />
        <Roadmap />
        <Book
          body="Build a feedback loop with BAML in a live walkthrough, or bring your stack to an engineering session with our team."
          heading="Get started with Boundary"
        />
      </main>
      <Footer />
      <SectionViews />
      <CtaClicks />
    </div>
  );
}
