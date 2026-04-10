'use client';

import { ChevronRightIcon } from '@radix-ui/react-icons';
import Link from 'next/link';
import { siteConfig } from '@/app/_lib/config';
import { useMediaQuery } from '@/hooks/use-media-query';
import { FlickeringGrid } from './magicui/flickering-grid';

export function FooterSection() {
  const tablet = useMediaQuery({ query: '(max-width: 1024px)' });
  const mobile = useMediaQuery({ query: '(max-width: 768px)' });
  const fontSize = mobile ? 70 : tablet ? 90 : 160;

  return (
    <footer className="w-full pb-0" id="footer">
      <div className="grid grid-cols-1 md:grid-cols-[minmax(0,320px)_1fr] gap-y-10 md:gap-x-16 p-10 items-start">
        <div className="flex flex-col items-start gap-y-5 max-w-xs mx-0">
          <Link className="flex items-center gap-2" href="/">
            {/* <Icons.logo className="size-12" /> */}
            <p className="text-xl font-semibold text-primary">Boundary</p>
          </Link>
          <p className="tracking-tight text-muted-foreground font-medium">
            {siteConfig.hero.description}
          </p>
          <div className="flex items-center gap-2 dark:hidden">
            {/* <Icons.soc2 className="size-12" /> */}
            {/* <Icons.hipaa className="size-12" /> */}
            {/* <Icons.gdpr className="size-12" /> */}
          </div>
          <div className="dark:flex items-center gap-2 hidden">
            {/* <Icons.soc2Dark className="size-12" /> */}
            {/* <Icons.hipaaDark className="size-12" /> */}
            {/* <Icons.gdprDark className="size-12" /> */}
          </div>
        </div>
        <div className="w-full">
          <div className="grid grid-cols-2 xl:grid-cols-4 gap-y-6 gap-x-8">
            {siteConfig.footerLinks.map((column) => (
              <ul className="flex flex-col gap-y-2" key={column.title}>
                <li className="mb-2 text-sm font-semibold text-primary">
                  {column.title}
                </li>
                {column.links.map((link) => (
                  <li
                    className="group inline-flex cursor-pointer items-center justify-start gap-1 text-[15px]/snug text-muted-foreground"
                    key={link.id}
                  >
                    <Link href={link.url}>{link.title}</Link>
                    <div className="flex size-4 items-center justify-center border border-border rounded translate-x-0 transform opacity-0 transition-all duration-300 ease-out group-hover:translate-x-1 group-hover:opacity-100">
                      <ChevronRightIcon className="size-4" />
                    </div>
                  </li>
                ))}
              </ul>
            ))}
          </div>
        </div>
      </div>
      <div className="w-full h-48 md:h-64 relative z-0">
        <div className="absolute inset-0 bg-gradient-to-t from-transparent to-background z-10 from-40%" />
        <div className="absolute inset-0 mx-6">
          <FlickeringGrid
            className="h-full w-full"
            color="#6B7280"
            flickerChance={0.1}
            fontSize={fontSize}
            gridGap={tablet ? 2 : 3}
            maxOpacity={0.3}
            squareSize={2}
            text={'Boundary'}
          />
        </div>
      </div>
    </footer>
  );
}
