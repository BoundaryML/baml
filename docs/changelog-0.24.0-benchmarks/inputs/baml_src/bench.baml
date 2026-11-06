// The request line an HTTP client builds, as one backtick template with
// string and int holes, 10,000 times.
function template_bench() -> int {
  let names = ["apple", "pear", "plum", "fig", "kiwi"];
  let families = ["FR", "VG", "DY"];
  let limit = 25;
  let host = "api.example.com";
  let seed = 7;
  let total = 0;
  for (let i = 0; i < 10000; i += 1) {
    seed = seed * 16807 % 2147483647;
    let org = families[i % 3];
    let item = names[seed % 5];
    let id = seed % 100000;
    let page = i % 50;
    let s = `GET /v1/orgs/${org}/items/${item}/${id}?page=${page}&limit=${limit}&sort=${item}&order=${org} host=${host} user=${item}-${id} attempt=${page} region=${org}`;
    total += s.length();
  }
  total
}

// The same request line as `template-request-line-10k`, built by joining a
// 24-element array of strings with `join("")`, 10,000 times.
function join_bench() -> int {
  let names = ["apple", "pear", "plum", "fig", "kiwi"];
  let families = ["FR", "VG", "DY"];
  let limit = 25;
  let host = "api.example.com";
  let seed = 7;
  let total = 0;
  for (let i = 0; i < 10000; i += 1) {
    seed = seed * 16807 % 2147483647;
    let org = families[i % 3];
    let item = names[seed % 5];
    let id = seed % 100000;
    let page = i % 50;
    let s = ["GET /v1/orgs/", org, "/items/", item, "/", id.to_string(), "?page=", page.to_string(), "&limit=", limit.to_string(), "&sort=", item, "&order=", org, " host=", host, " user=", item, "-", id.to_string(), " attempt=", page.to_string(), " region=", org].join("");
    total += s.length();
  }
  total
}

// Enum `==` the way a state machine uses it: each step compares a task's
// status against variants to pick the next one. 1,000 tasks, 40 rounds,
// about 110,000 comparisons.
enum Status {
  Todo
  Doing
  Review
  Done
}

class Task {
  id: int
  status: Status
}

function enum_advance(s: Status, n: int) -> Status {
  if (s == Status.Todo) { return Status.Doing; }
  if (s == Status.Doing) {
    if (n % 3 == 0) { return Status.Review; }
    return Status.Doing;
  }
  if (s == Status.Review) {
    if (n % 5 == 0) { return Status.Doing; }
    return Status.Done;
  }
  Status.Todo
}

function enum_bench() -> int {
  let tasks: Task[] = [];
  for (let i = 0; i < 1000; i += 1) {
    tasks.push(Task { id: i, status: Status.Todo });
  }
  let done = 0;
  for (let round = 0; round < 40; round += 1) {
    for (let t in tasks) {
      t.status = enum_advance(t.status, t.id + round);
      if (t.status == Status.Done) { done += t.id % 7 + 1; }
    }
  }
  done
}

// Benchmarks Game n-body: five-body Jovian simulation, symplectic integrator.
// Float arithmetic, sqrt, and mutable fields on a small array of objects.
class Body {
  x: float
  y: float
  z: float
  vx: float
  vy: float
  vz: float
  mass: float
}
function body(x: float, y: float, z: float, vx: float, vy: float, vz: float, mass: float) -> Body {
  let pi = 3.141592653589793;
  let solar_mass = 4.0 * pi * pi;
  let days = 365.24;
  Body { x: x, y: y, z: z, vx: vx * days, vy: vy * days, vz: vz * days, mass: mass * solar_mass }
}
function advance(bodies: Body[], dt: float) -> int {
  let n = bodies.length();
  for (let i = 0; i < n; i += 1) {
    let a = bodies[i];
    for (let j = i + 1; j < n; j += 1) {
      let b = bodies[j];
      let dx = a.x - b.x;
      let dy = a.y - b.y;
      let dz = a.z - b.z;
      let d2 = dx * dx + dy * dy + dz * dz;
      let mag = dt / (d2 * d2.sqrt());
      let am = a.mass * mag;
      let bm = b.mass * mag;
      a.vx -= dx * bm;
      a.vy -= dy * bm;
      a.vz -= dz * bm;
      b.vx += dx * am;
      b.vy += dy * am;
      b.vz += dz * am;
    }
  }
  for (let b in bodies) {
    b.x += dt * b.vx;
    b.y += dt * b.vy;
    b.z += dt * b.vz;
  }
  0
}
function energy(bodies: Body[]) -> float {
  let e = 0.0;
  let n = bodies.length();
  for (let i = 0; i < n; i += 1) {
    let a = bodies[i];
    e += 0.5 * a.mass * (a.vx * a.vx + a.vy * a.vy + a.vz * a.vz);
    for (let j = i + 1; j < n; j += 1) {
      let b = bodies[j];
      let dx = a.x - b.x;
      let dy = a.y - b.y;
      let dz = a.z - b.z;
      e -= a.mass * b.mass / (dx * dx + dy * dy + dz * dz).sqrt();
    }
  }
  e
}
function nbody_bench() -> int {
  let bodies = [
    body(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0),
    body(4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01, 1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04),
    body(8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01, -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04),
    body(1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01, 2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05),
    body(1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01, 2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05),
  ];
  let pi = 3.141592653589793;
  let solar_mass = 4.0 * pi * pi;
  let px = 0.0;
  let py = 0.0;
  let pz = 0.0;
  for (let b in bodies) {
    px += b.vx * b.mass;
    py += b.vy * b.mass;
    pz += b.vz * b.mass;
  }
  let sun = bodies[0];
  sun.vx = -px / solar_mass;
  sun.vy = -py / solar_mass;
  sun.vz = -pz / solar_mass;
  let before = energy(bodies);
  for (let step = 0; step < 20000; step += 1) {
    advance(bodies, 0.01);
  }
  let after = energy(bodies);
  (before * 1000000000.0).itrunc() + (after * 1000000000.0).itrunc()
}

function noop() -> int { 1 }
