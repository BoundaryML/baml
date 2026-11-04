import Adopt from '@/components/home/adopt';
import Book from '@/components/home/book';
import Deploy from '@/components/home/deploy';
import Footer from '@/components/home/footer';
import ForceDark from '@/components/home/force-dark';
import Hero from '@/components/home/hero';
import Improve from '@/components/home/improve';
import Language from '@/components/home/language';
import Logos from '@/components/home/logos';
import Nav from '@/components/home/nav';
import Observe from '@/components/home/observe';
import Statement from '@/components/home/statement';
import { SectionViews } from '@/components/home/track';
import Workflow from '@/components/home/workflow';
import './styles/v2.css';
import './styles/diagram-styles.css';
import './styles/v3.css';
import './styles/home.css';

// The boundaryml.com homepage. It sells the whole stack, and every section leads to booking a call.

export default function Home() {
  return (
    <div className="v2 v3-page v4-page min-h-screen" data-theme="dark">
      <ForceDark />
      <Nav />
      <main id="main">
        <Hero />
        <Logos />
        <Statement />
        <Workflow />
        <Language />
        <Deploy />
        <Observe />
        <Improve />
        <Adopt />
        <Book />
      </main>
      <Footer />
      <SectionViews />
    </div>
  );
}
