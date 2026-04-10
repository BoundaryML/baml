'use client';

import { useTheme } from 'next-themes';
import Image from 'next/image';

const BRANDFETCH_CLIENT_ID = '1idQbe1D_SxVi_WjGRi';

const customerLogos = [
  { alt: 'Product Hunt', src: '/testimonials/logos/product-hunt.png', type: 'local' as const },
  { alt: 'SAP', src: '/testimonials/logos/sapLogo.png', type: 'local' as const },
  { alt: 'EY', src: '/testimonials/logos/ey.png', type: 'local' as const },
  { alt: 'AMD', src: '/testimonials/logos/amd.png', type: 'local' as const },
  { alt: 'Vetrec', src: '/testimonials/logos/vetrec.png', type: 'local' as const },
  { alt: 'AWS', src: '/testimonials/logos/aws.png', type: 'local' as const },
  { alt: 'Cisco', src: '/testimonials/logos/cisco.png', type: 'local' as const },
];

function LogoCell({ alt, src }: { alt: string; src: string }) {
  return (
    <div className="relative w-24 h-10 sm:w-28 sm:h-12 flex-shrink-0">
      <Image
        alt={alt}
        className="object-contain grayscale opacity-55 hover:grayscale-0 hover:opacity-100 transition-all duration-300"
        fill
        priority
        sizes="112px"
        src={src}
      />
    </div>
  );
}

export function CompanyShowcase() {
  const theme = useTheme();
  const isDark = theme.theme === 'dark';

  const logosWithUrls = customerLogos.map((logo) => {
    if (logo.type === 'brandfetch') {
      const themeSegment = isDark ? 'theme/light' : 'theme/dark';
      return {
        alt: logo.alt,
        src: `https://cdn.brandfetch.io/${logo.domain}/w/256/h/128/${themeSegment}/symbol?c=${BRANDFETCH_CLIENT_ID}`,
      };
    }
    return { alt: logo.alt, src: logo.src };
  });

  return (
    <section
      className="flex flex-col items-center justify-center gap-8 py-10 w-full relative sm:py-12"
      id="company"
    >
      <p className="text-xs font-medium uppercase tracking-[0.2em] text-muted-foreground">
        Trusted by developers at
      </p>
      <div className="flex justify-between items-center w-full max-w-7xl px-8 sm:px-16">
        {logosWithUrls.map((logo) => (
          <LogoCell key={logo.alt} alt={logo.alt} src={logo.src} />
        ))}
      </div>
    </section>
  );
}
