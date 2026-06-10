'use client';

import { useState } from 'react';
import { SiPython, SiTypescript } from 'react-icons/si';
import { BamlCode } from '../../learn2/_components/BamlCode';

/*
 * One BAML class on the left; on the right, what the generated SDK gives
 * each host language — switched with tabs (learn4's RotatingTypes, minus
 * the auto-rotation). Pane contents track the real generator output
 * (pydantic v2 BaseModel / export class with a typed constructor).
 */

const BAML_SIDE = `class Greeting {
  message: string,
  letters: int,

  function shout(self) -> string {
    self.message.to_upper_case()
  }
}`;

const PY_SIDE = `# generated: baml_sdk (pydantic v2)
class Greeting(pydantic.BaseModel):
    message: str
    letters: int

    def shout(self) -> str: ...   # the BAML method, bound

g = Greeting.new("hn")            # BAML factory, from python
g.shout()                         # "HI, HN"
g.model_dump_json()               # plain pydantic underneath`;

const TS_SIDE = `// generated: baml_sdk (typescript/node)
export class Greeting {
  message!: string;
  letters!: number;
  constructor(init: { message: string; letters: number }) {
    Object.assign(this, init);
  }
  shout(): string;                // the BAML method, bound
}

const g = Greeting.new('hn');     // same factory, from ts
g.shout();                        // "HI, HN"`;

export function TypeTabs() {
  const [lang, setLang] = useState<'python' | 'typescript'>('python');
  return (
    <div>
      <div aria-label="Host language" className="l6-sdk-tabs" role="tablist">
        <button
          aria-selected={lang === 'python'}
          className={`l6-sdk-tab font-mono${lang === 'python' ? ' l6-sdk-tab--on' : ''}`}
          onClick={() => setLang('python')}
          role="tab"
          type="button"
        >
          <SiPython aria-hidden color="#3776AB" size={14} />
          Python
        </button>
        <button
          aria-selected={lang === 'typescript'}
          className={`l6-sdk-tab font-mono${lang === 'typescript' ? ' l6-sdk-tab--on' : ''}`}
          onClick={() => setLang('typescript')}
          role="tab"
          type="button"
        >
          <SiTypescript aria-hidden color="#3178C6" size={14} />
          TypeScript
        </button>
      </div>
      <div className="l6-pair">
        <div>
          <p className="l6-pane-label">one baml class</p>
          <BamlCode code={BAML_SIDE} filename="greeting.baml" />
        </div>
        {lang === 'python' ? (
          <div>
            <p className="l6-pane-label l6-pane-label--after">
              what python sees — pydantic
            </p>
            <BamlCode code={PY_SIDE} filename="baml_sdk" lang="python" />
          </div>
        ) : (
          <div>
            <p className="l6-pane-label l6-pane-label--after">
              what typescript sees
            </p>
            <BamlCode code={TS_SIDE} filename="baml_sdk" lang="typescript" />
          </div>
        )}
      </div>
    </div>
  );
}
