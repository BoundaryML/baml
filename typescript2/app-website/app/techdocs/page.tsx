import { createMetadata } from '@/app/_lib/metadata';
import { FooterSection } from '@/components/footer-section';
import { Navbar } from '@/components/navbar';

// Landing spot for every "Read more" link on the homepage until the real
// deep-dive articles ship. One page, one promise: the writing is human.

export const metadata = createMetadata({
  description:
    'BAML deep-dive articles are coming, written by humans, strictly no AI.',
  ogTitle: 'Tech docs',
  path: '/techdocs',
  title: 'Tech docs',
});

const CSS = `
.nolarp {
  margin: 0 auto; max-width: 680px;
  padding: 96px 24px 128px;
  color: #1A1612; font-size: 17px; line-height: 1.7;
}
.nolarp h1 {
  font-size: clamp(30px, 4.5vw, 42px); letter-spacing: -0.02em;
  line-height: 1.1; margin: 0 0 28px;
}
.nolarp p { margin: 0 0 20px; }
`;

export default function NoLarpPage() {
  return (
    <>
      {/* biome-ignore lint/security/noDangerouslySetInnerHtml: static page CSS */}
      <style dangerouslySetInnerHTML={{ __html: CSS }} />
      <Navbar />
      <main className="nolarp">
        <h1>Tech Docs Coming Soon</h1>
        <p>
          We know there are a lot of toy &ldquo;programming languages&rdquo;
          coming out these days. We&rsquo;ve been building BAML for a while. We
          know our stuff. These articles are strictly no-AI. We explain our type
          system, our observability features, and more.
        </p>
      </main>
      <FooterSection />
    </>
  );
}
