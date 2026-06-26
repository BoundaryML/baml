import WarScene from '@/components/WarScene';
import PledgeForm from '@/components/PledgeForm';
import PledgeWall from '@/components/PledgeWall';

export default function Home() {
  return (
    <main className="w-full">
      {/* Call to action + form */}
      <section className="mx-auto max-w-2xl px-6 pt-10 pb-6">
        <p
          className="text-center text-xl leading-relaxed text-black sm:text-2xl"
          style={{ fontFamily: "'Times New Roman', Times, serif" }}
        >
          fill out the form to share how you are fighting slop with slop
        </p>
        <div className="mt-8">
          <PledgeForm />
        </div>
      </section>

      {/* Live wall of pledges */}
      <section className="pt-2 pb-3">
        <PledgeWall />
      </section>

      {/* The march against slop — cycling battlegrounds */}
      <div className="h-56 w-full overflow-hidden sm:h-64">
        <WarScene />
      </div>
    </main>
  );
}
