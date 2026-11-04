import { section } from '../analytics';
import Blueprints from './blueprints';

/** Section 2: why BAML is a language, from the founders, beside a blueprint: three Raptor engines, or the layers of the stack. */
export default function Statement() {
  return (
    <section
      {...section('why')}
      aria-labelledby="why"
      className="border-t border-rule"
    >
      <div className="v3-wrap mx-auto px-(--frame-pad) py-20 md:py-28">
        {/* The heading runs across the whole section, over both the text and the drawing. */}
        <h2 className="v4-h2" id="why">
          Iteration loops are <em className="pr-[0.08em]">fast</em> when
          everything speaks the same language.
        </h2>
        <div className="mt-12 md:mt-16 grid lg:grid-cols-[minmax(0,1fr)_minmax(0,1.05fr)] gap-x-16 gap-y-12 items-center">
          <div>
            <p className="font-display font-medium text-[24px] md:text-[30px] leading-[1.2] tracking-[-0.02em] text-balance text-head">
              We didn't want to build a programming language.
            </p>
            <div className="mt-6 flex flex-col gap-5 max-w-[36rem] v4-body text-ink-2">
              <p>
                But agents today wait on slow CI and juggle separate tools to
                run experiments, check alerts, build dashboards, and work out
                which version of the code did what.
              </p>
              <p>
                To make something 100x better, you have to vertically integrate.
                So we built a virtual machine that runs anywhere, a compiler
                that knows which errors you're hitting in production, and a data
                lake that stores every call in a structured way, even with the
                types you declared.
              </p>
            </div>
          </div>
          <Blueprints />
        </div>
      </div>
    </section>
  );
}
