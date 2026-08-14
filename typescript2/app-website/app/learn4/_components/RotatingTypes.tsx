'use client';

import { BamlCode } from '../../learn2/_components/BamlCode';

/*
 * One BAML class on the left; on the right, what the generated SDK gives
 * each host language — crossfading between Python and TypeScript on a CSS
 * timer (hover to pause). Pane contents track the real generator output
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

export function RotatingTypes() {
  return (
    <div className="l4-pair">
      <div>
        <p className="l4-pane-label">one baml class</p>
        <BamlCode filename="greeting.baml" code={BAML_SIDE} />
      </div>
      <div className="l4-rot">
        <div>
          <p className="l4-pane-label l4-pane-label--after">
            what python sees — pydantic
          </p>
          <BamlCode lang="python" filename="baml_sdk" code={PY_SIDE} />
        </div>
        <div>
          <p className="l4-pane-label l4-pane-label--after">
            what typescript sees
          </p>
          <BamlCode lang="typescript" filename="baml_sdk" code={TS_SIDE} />
        </div>
      </div>
    </div>
  );
}
