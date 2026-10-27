// A generic decoder called through the interface's static method: the shape
// of a library helper `parse<T extends FromJson>(j)` that a program calls for
// each record type it reads. `Point` and `Reading` provide their own
// `from_json`; each call resolves `T`'s impl. 20,000 decodes.
class Point {
  x: int
  y: int
  implements baml.FromJson {
    function from_json(j: baml.json.json) -> Point throws baml.json.DecodeError {
      let xs = baml.json.to<int[]>(j);
      Point { x: xs[0], y: xs[1] }
    }
  }
}

class Reading {
  sensor: string
  value: int
  implements baml.FromJson {
    function from_json(j: baml.json.json) -> Reading throws baml.json.DecodeError {
      let parts = baml.json.to<string>(j).split(":");
      Reading { sensor: parts[0], value: int.parse(parts[1]) catch (_) { let _ => 0 } }
    }
  }
}

function parse<T extends baml.FromJson>(j: baml.json.json) -> T throws baml.json.DecodeError {
  T.from_json(j)
}

function main() -> int throws baml.json.DecodeError {
  let seed = 3;
  let acc = 0;
  for (let i = 0; i < 10000; i += 1) {
    seed = seed * 16807 % 2147483647;
    let p = parse<Point>([seed % 1000, seed % 777]);
    let r = parse<Reading>("s" + (seed % 10).to_string() + ":" + (seed % 50).to_string());
    acc += p.x * 3 + p.y + r.value + r.sensor.length();
  }
  acc
}
