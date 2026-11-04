import { section } from '../analytics';
import { Button, LINKS } from '../ui';
import { EXPLORE } from '../v2/links';
import LoopDiagrams from './loop-diagrams';

export default function Hero() {
  return (
    <section
      {...section('hero')}
      aria-labelledby="hero-title"
      className="relative overflow-hidden"
    >
      <div className="relative v3-wrap mx-auto px-(--frame-pad) pt-14 md:pt-24">
        <LoopDiagrams
          intro={
            <>
              <h1 className="v3-h1 max-w-[12.5em] text-balance" id="hero-title">
                The programming language for agents
              </h1>
              <div className="flex flex-wrap items-end justify-between gap-x-12 gap-y-6 mt-5 md:mt-7">
                <p className="max-w-[30rem] text-base md:text-lg leading-[1.45] tracking-[-0.01em] text-white/70">
                  BAML lets agents close the development loop. Build, test,
                  trace, query, and run experiments with a unified, vertically
                  integrated toolchain.
                </p>
                <div className="flex flex-wrap gap-2.5">
                  <Button href={LINKS.quickstart}>Install BAML</Button>
                  <Button href={EXPLORE} variant="secondary">
                    Explore BAML
                  </Button>
                </div>
              </div>
            </>
          }
        />
      </div>
    </section>
  );
}
