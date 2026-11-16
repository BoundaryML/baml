import lambWhite from '@boundaryml/brand/logo/svg/baml-logomark-white.svg';
import Image from 'next/image';
import { section } from './analytics';
import { SIGNUP } from './links';
import { Button } from './ui';

/** The homepage and blog's closing link to early access. */
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
            When humans wrote code, it was okay to move in days or weeks.
          </h2>
          <p className="mt-6 v4-body text-ink">
            Now AI writes code.{' '}
            <span className="text-clover">Hyperobservability</span> gets you the
            data and the speed. Making great decisions is still your job.
          </p>
          <div className="mt-8">
            <Button href={SIGNUP}>Get early access</Button>
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
