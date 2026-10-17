import math
PI = 3.141592653589793
SOLAR_MASS = 4.0 * PI * PI
DAYS = 365.24

class Body:
    __slots__ = ('x', 'y', 'z', 'vx', 'vy', 'vz', 'mass')

    def __init__(self, x, y, z, vx, vy, vz, mass):
        self.x = x
        self.y = y
        self.z = z
        self.vx = vx * DAYS
        self.vy = vy * DAYS
        self.vz = vz * DAYS
        self.mass = mass * SOLAR_MASS

def advance(bodies, dt):
    n = len(bodies)
    for i in range(n):
        a = bodies[i]
        for j in range(i + 1, n):
            b = bodies[j]
            dx = a.x - b.x
            dy = a.y - b.y
            dz = a.z - b.z
            d2 = dx * dx + dy * dy + dz * dz
            mag = dt / (d2 * math.sqrt(d2))
            am = a.mass * mag
            bm = b.mass * mag
            a.vx -= dx * bm
            a.vy -= dy * bm
            a.vz -= dz * bm
            b.vx += dx * am
            b.vy += dy * am
            b.vz += dz * am
    for b in bodies:
        b.x += dt * b.vx
        b.y += dt * b.vy
        b.z += dt * b.vz

def energy(bodies):
    e = 0.0
    n = len(bodies)
    for i in range(n):
        a = bodies[i]
        e += 0.5 * a.mass * (a.vx * a.vx + a.vy * a.vy + a.vz * a.vz)
        for j in range(i + 1, n):
            b = bodies[j]
            dx = a.x - b.x
            dy = a.y - b.y
            dz = a.z - b.z
            e -= a.mass * b.mass / math.sqrt(dx * dx + dy * dy + dz * dz)
    return e
bodies = [Body(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0), Body(4.841431442464721, -1.1603200440274284, -0.10362204447112311, 0.001660076642744037, 0.007699011184197404, -6.90460016972063e-05, 0.0009547919384243266), Body(8.34336671824458, 4.124798564124305, -0.4035234171143214, -0.002767425107268624, 0.004998528012349172, 2.3041729757376393e-05, 0.0002858859806661308), Body(12.894369562139131, -15.111151401698631, -0.22330757889265573, 0.002964601375647616, 0.0023784717395948095, -2.9658956854023756e-05, 4.366244043351563e-05), Body(15.379697114850917, -25.919314609987964, 0.17925877295037118, 0.0026806777249038932, 0.001628241700382423, -9.515922545197159e-05, 5.1513890204661145e-05)]

def main():
    PI = 3.141592653589793
    px = py = pz = 0.0
    for b in bodies:
        px += b.vx * b.mass
        py += b.vy * b.mass
        pz += b.vz * b.mass
    bodies[0].vx = -px / SOLAR_MASS
    bodies[0].vy = -py / SOLAR_MASS
    bodies[0].vz = -pz / SOLAR_MASS
    before = energy(bodies)
    for _ in range(20000):
        advance(bodies, 0.01)
    after = energy(bodies)
    return int(before * 1000000000.0) + int(after * 1000000000.0)
