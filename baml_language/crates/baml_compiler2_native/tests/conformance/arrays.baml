// Shared by the cases of the array natives: arrays built at run time.

class Named {
  name: string
  rank: int
}
function ints(n: int) -> int[] {
  let out: int[] = [];
  for (let i = 0; i < n; i += 1) { out.push(i * 10); };
  out
}
function floats(n: int) -> float[] {
  let out: float[] = [];
  for (let i = 0; i < n; i += 1) { out.push(i + 0.5); };
  out
}
function words(n: int) -> string[] {
  let out: string[] = [];
  for (let i = 0; i < n; i += 1) { out.push(rep("w", i + 1)); };
  out
}
function people(n: int) -> Named[] {
  let out: Named[] = [];
  for (let i = 0; i < n; i += 1) { out.push(Named { name: rep("p", i + 1), rank: i }); };
  out
}
function show(xs: int[]) -> string {
  let out = `${xs.length()}:`;
  for (let x in xs) { out = `${out}${x},`; };
  out
}
function show_words(xs: string[]) -> string {
  let out = `${xs.length()}:`;
  for (let x in xs) { out = `${out}${x},`; };
  out
}
function ranks(xs: Named[]) -> string {
  let out = `${xs.length()}:`;
  for (let x in xs) { out = `${out}${x.name}${x.rank},`; };
  out
}
