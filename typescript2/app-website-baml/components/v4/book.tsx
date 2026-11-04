import lambWhite from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import Image from 'next/image';
import { section } from '../analytics';
import { Button } from '../ui';
import { BOOK } from './links';

/** Section 7: the same booking link as the nav and the hero, over the clover field. */
export default function Book() {
  return (
    <section
      {...section('book')}
      aria-labelledby="book"
      className="on-paint relative overflow-hidden"
    >
      <Image
        alt=""
        className="object-cover object-[50%_70%]"
        fill
        quality={90}
        sizes="100vw"
        src="/paint/baml-dusk-field.jpg"
      />
      <div className="absolute inset-0 bg-black/60" />
      <div className="relative v3-wrap mx-auto px-(--frame-pad) py-20 md:py-32 flex max-md:flex-col max-md:items-start items-center justify-between gap-10">
        <div className="max-w-[560px]">
          <h2 className="v4-h2" id="book">
            Get access to the BAML stack.
          </h2>
          <p className="mt-6 text-lg leading-7 text-white/85">
            The stack is in private beta. Book a 45-minute call with the
            founders, and we'll set you up.
          </p>
          <div className="mt-8">
            <Button href={BOOK}>Book a call</Button>
          </div>
        </div>
        <Image
          alt=""
          className="shrink-0 w-[88px] md:w-[clamp(140px,14vw,200px)] h-auto max-md:order-first"
          height={285}
          src={lambWhite}
          width={240}
        />
      </div>
    </section>
  );
}
