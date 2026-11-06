class State { scratch: map<string,int>, previous: int? }
function main() -> int {
  let state = State {scratch: {}, previous: null};
  state.scratch["a"] = 10;
  state.scratch["b"] = 20;
  state.previous = state.scratch.set("a", 30);
  let values = state.scratch.values();
  state.scratch["b"] = 40;
  (state.previous ?? 99) * 100 + values[0] + values[1]
}
