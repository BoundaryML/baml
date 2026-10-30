import Image from 'next/image';
import { section } from '../analytics';

const LOGOS: {
  name: string;
  src: string;
  h: number;
  w: number;
  light?: boolean;
}[] = [
  { h: 15, name: 'Stitch Fix', src: '/logos/stitchfix.png', w: 133 },
  { h: 19, light: true, name: 'Feather', src: '/logos/feather.png', w: 104 },
  { h: 28, name: 'SAP', src: '/logos/sap.svg', w: 57 },
  { h: 20, name: 'AMD', src: '/logos/amd.svg', w: 84 },
  { h: 31, name: 'EY', src: '/logos/ey.svg', w: 31 },
  { h: 27, name: 'Akamai', src: '/logos/akamai.svg', w: 66 },
  { h: 20, name: 'Vetcove', src: '/logos/vetcove.svg', w: 89 },
];

function Row({ copy }: { copy?: boolean }) {
  return (
    <ul
      aria-hidden={copy}
      className={`flex items-center shrink-0 ${copy ? 'v2-marquee-copy' : ''}`}
    >
      {LOGOS.map((l) => (
        <li className="px-8 md:px-12" key={l.name}>
          <Image
            alt={copy ? '' : l.name}
            className={`${l.light ? 'logo-mark-light' : 'logo-mark'} w-auto max-w-none opacity-70`}
            height={l.h}
            src={l.src}
            style={{ height: l.h * 1.2 }}
            width={l.w}
          />
        </li>
      ))}
    </ul>
  );
}

export default function Logos() {
  return (
    <section
      {...section('logos')}
      aria-label="Used by engineers at"
      className="border-y border-white/12 py-9 md:py-11"
    >
      <p className="mb-7 text-center text-[15px] text-white/60">
        Used by engineers at
      </p>
      <div className="v2-marquee">
        <div className="v2-marquee-track">
          <Row />
          <Row copy />
        </div>
      </div>
    </section>
  );
}
