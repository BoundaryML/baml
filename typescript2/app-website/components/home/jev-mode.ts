// biome-ignore-all lint/suspicious/noTemplateCurlyInString: BAML source, whose template strings use ${}.

// Jev mode: .feels() on every value, from one interface, one blanket implementation, and one Jev
// function. Compiles on its own and is laid out as baml fmt writes it.
export const JEV_MODE = [
  '// Every value can be asked how it feels.',
  'function inbox(email: string) -> string {',
  '    if (email.feels("urgent")) {',
  '        "page on-call"',
  '    } else {',
  '        "later"',
  '    }',
  '}',
  '',
  '// .feels() comes from one blanket implementation, for every type.',
  'interface Vibes {',
  '    function feels(self, quality: string) -> bool throws unknown;',
  '}',
  '',
  'implements<S> Vibes for S {',
  '    function feels(self, quality: string) -> bool {',
  '        Feels(baml.json.stringify(self), quality)',
  '    }',
  '}',
  '',
  'function Feels(state: string, quality: string) -> bool {',
  '    client: typesafeai.Client.new(model = "jev-latest")',
  '    prompt: `',
  '        Does this feel ${quality}?',
  '        ${state}',
  '    `',
  '}',
];
