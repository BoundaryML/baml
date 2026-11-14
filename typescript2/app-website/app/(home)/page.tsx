import Adopt from '@/components/home/adopt';
import Agents from '@/components/home/agents';
import Book from '@/components/home/book';
import Footer from '@/components/home/footer';
import ForceDark from '@/components/home/force-dark';
import Hero from '@/components/home/hero';
import Hyperobs from '@/components/home/hyperobs';
import Improve from '@/components/home/improve';
import Language from '@/components/home/language';
import Logos from '@/components/home/logos';
import Nav from '@/components/home/nav';
import Observe from '@/components/home/observe';
import Review from '@/components/home/review';
import SeenEnough from '@/components/home/seen-enough';
import Software from '@/components/home/software';
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
      <Nav />
      <main id="main">
        <Hero />
        <Logos />
        <Hyperobs>
          <Observe />
        </Hyperobs>
        <Agents />
        <Stack />
        <Language />
        <Review />
        <Software />
        <Improve />
        <Adopt />
        <Book />
      </main>
      <Footer />
      <SeenEnough />
      <SectionViews />
      <CtaClicks />
    </div>
  );
}
