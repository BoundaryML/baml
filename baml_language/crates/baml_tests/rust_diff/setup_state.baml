// main() deliberately differs: benchmark jobs must use run(setup()), not main().
class State { payload string stamp int }
function setup() -> State { State { payload: "retained", stamp: 79 } }
function run(state: State) -> int {
  let sum = 0;
  for (let i = 0; i < 256; i += 1) {
    let garbage = State { payload: "garbage", stamp: i };
    sum += garbage.stamp;
  }
  state.payload.length() + state.stamp + sum % 7
}
function main() -> int { 777 }
