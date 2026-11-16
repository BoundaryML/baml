import lambWhite from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import type { Metadata } from 'next';
import Image from 'next/image';
import { PAGE_WRAP, PageShell } from '@/components/home/page-shell';
import { Button } from '@/components/home/ui';
import { FONT_VARIABLES } from './(home)/fonts';
import './(home)/globals.css';

export const metadata: Metadata = {
  title: 'Page not found · Boundary',
};

/** The 404 page. `experimental.globalNotFound` is enabled, so this renders the whole document for
 *  unmatched routes rather than going through the root layout. It loads the same fonts and stylesheet
 *  as the root layout and sits in the homepage's nav and footer. */
export default function GlobalNotFound() {
  return (
    <html
      className={`${FONT_VARIABLES} antialiased`}
      data-theme="dark"
      lang="en"
    >
      <body className="min-h-full flex flex-col">
        <PageShell>
          <section
            className={`${PAGE_WRAP} py-24 md:py-36 flex flex-col items-start`}
          >
            <Image
              alt=""
              className="h-16 w-auto"
              height={285}
              src={lambWhite}
              width={240}
            />
            <p className="mt-10 v4-caption text-muted">404</p>
            <h1 className="mt-4 v4-h1 max-w-[12em]">
              Looks like you wandered from the flock.
            </h1>
            <div className="mt-10">
              <Button href="/">Back to the homepage</Button>
            </div>
          </section>
        </PageShell>
      </body>
    </html>
  );
}
