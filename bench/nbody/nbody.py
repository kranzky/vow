# n-body: the same algorithm as nbody.vow.
import math
import sys

PI = 3.141592653589793
SOLAR_MASS = 4 * PI * PI
DAYS_PER_YEAR = 365.24
N = 5

B = [
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
    [4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
     1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04],
    [8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
     -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04],
    [1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
     2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05],
    [1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
     2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05],
]
x = [b[0] for b in B]
y = [b[1] for b in B]
z = [b[2] for b in B]
vx = [b[3] * DAYS_PER_YEAR for b in B]
vy = [b[4] * DAYS_PER_YEAR for b in B]
vz = [b[5] * DAYS_PER_YEAR for b in B]
mass = [b[6] * SOLAR_MASS for b in B]


def energy():
    e = 0.0
    for i in range(N):
        e += 0.5 * mass[i] * (vx[i] * vx[i] + vy[i] * vy[i] + vz[i] * vz[i])
        for j in range(i + 1, N):
            dx, dy, dz = x[i] - x[j], y[i] - y[j], z[i] - z[j]
            e -= mass[i] * mass[j] / math.sqrt(dx * dx + dy * dy + dz * dz)
    return e


def advance(dt):
    for i in range(N):
        for j in range(i + 1, N):
            dx, dy, dz = x[i] - x[j], y[i] - y[j], z[i] - z[j]
            d2 = dx * dx + dy * dy + dz * dz
            mag = dt / (d2 * math.sqrt(d2))
            mi, mj = mass[i] * mag, mass[j] * mag
            vx[i] -= dx * mj
            vy[i] -= dy * mj
            vz[i] -= dz * mj
            vx[j] += dx * mi
            vy[j] += dy * mi
            vz[j] += dz * mi
    for i in range(N):
        x[i] += dt * vx[i]
        y[i] += dt * vy[i]
        z[i] += dt * vz[i]


steps = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
px = sum(vx[i] * mass[i] for i in range(N))
py = sum(vy[i] * mass[i] for i in range(N))
pz = sum(vz[i] * mass[i] for i in range(N))
vx[0], vy[0], vz[0] = -px / SOLAR_MASS, -py / SOLAR_MASS, -pz / SOLAR_MASS
print(f"{energy():.9f}")
for _ in range(steps):
    advance(0.01)
print(f"{energy():.9f}")
