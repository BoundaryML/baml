import WarScene from '@/components/WarScene';
import PledgeForm from '@/components/PledgeForm';
import PledgeWall from '@/components/PledgeWall';

export default function Home() {
  return (
    <main className="min-h-screen w-full overflow-x-hidden">
      <section className="relative z-10 grid gap-8 px-4 pb-6 pt-5 sm:px-6 lg:grid-cols-2 lg:items-start lg:px-8 lg:pb-[40vw] lg:pt-7">
        <div className="min-w-0 overflow-hidden">
          <h1 className="text-2xl font-bold leading-none tracking-tight text-ink sm:text-4xl">
            fight slop with slop
          </h1>
          <p className="tweet-font mt-2 text-sm text-accent">
            by{' '}
            <a
              href="https://x.com/boundaryml"
              target="_blank"
              rel="noopener noreferrer"
              className="hover:underline"
            >
              @boundaryml
            </a>
          </p>
          <div className="mt-8">
            <PledgeWall />
          </div>
        </div>
        <div className="min-w-0 lg:mt-6 lg:pl-4 xl:pl-6">
          <PledgeForm />
        </div>
      </section>

      {/* The war band sits in normal flow on mobile (below the form, so it never
          covers it), and pins to the bottom of the viewport from `lg` up. Its
          height is keyed to viewport WIDTH (the full-bleed panorama is width/2.33
          tall): 38vw is always shorter than that, so the band crops a slice off
          the bottom foreground while keeping the full-width image and its top
          clouds — identically in every browser. Clamped so it never eats a tall
          page or vanishes on a very narrow one. */}
      <div className="relative h-[38vw] min-h-[120px] max-h-[80vh] w-full overflow-hidden lg:fixed lg:inset-x-0 lg:bottom-0">
        <WarScene />
      </div>
    </main>
  );
}
