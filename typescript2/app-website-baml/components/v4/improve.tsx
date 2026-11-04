import { RED } from '../v3/story';
import { Panel, Phase } from './phase';

// Section 7, Improve: the calls that show a problem become tests, and the fix goes to a share of
// traffic before everyone.

const PASTURE = 'var(--color-pasture-200)';

const OTHER = [
  'Oh great, charged twice again. Love that.',
  'Wow, the export broke again. Amazing.',
  'Sure, I will just wait another week.',
  'Fantastic, another invoice I never asked for.',
];

const VERSIONS = [
  { name: 'v43', share: '90% of traffic', value: 3.1 },
  { name: 'v44', share: '10% of traffic', value: 0.3 },
];

export default function Improve() {
  return (
    <Phase
      heading="Test and roll out fixes on real traffic."
      id="improve"
      intro="The calls that went wrong are stored with their real inputs, so you can test a fix against them before you ship it."
      name="Improve"
    >
      <div className="grid lg:grid-cols-2 gap-4">
        <Panel title="Tests from production">
          <div className="p-5 font-mono text-[12.5px] md:text-[13px] leading-[1.75]">
            <p className="text-white/40">31 calls labelled other</p>
            <div className="mt-2">
              {OTHER.map((input) => (
                <p className="flex gap-4" key={input}>
                  <span className="truncate text-white/70">{input}</span>
                  <span className="ml-auto shrink-0" style={{ color: RED }}>
                    other
                  </span>
                </p>
              ))}
              <p className="text-white/35">27 more</p>
            </div>
            <p className="mt-4 text-white/85 break-words">
              <span className="text-white/35">$ </span>
              baml test add classify --from-query "output_value = 'other'"
            </p>
            <p style={{ color: PASTURE }}>31 tests added</p>
          </div>
        </Panel>
        <Panel title="Ship to 10% first">
          <div className="p-5 font-mono text-[12.5px] md:text-[13px]">
            <p className="text-white/40">Share labelled other, by version</p>
            <div className="mt-5 grid grid-cols-[auto_minmax(0,1fr)] gap-x-5 gap-y-5 items-center">
              {VERSIONS.map((v) => {
                const color = v.value > 2 ? RED : PASTURE;
                return (
                  <div className="contents" key={v.name}>
                    <div>
                      <p className="text-white/85">{v.name}</p>
                      <p className="text-[11.5px] text-white/40">{v.share}</p>
                    </div>
                    <div className="flex items-center gap-3">
                      <span
                        className="h-3 rounded-[2px]"
                        style={{
                          background: color,
                          width: `${(v.value / 3.5) * 100}%`,
                        }}
                      />
                      <span className="tabular-nums" style={{ color }}>
                        {v.value}%
                      </span>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        </Panel>
      </div>
    </Phase>
  );
}
