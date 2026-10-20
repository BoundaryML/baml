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
function main() -> int {
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
