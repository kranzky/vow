// n-body: the same algorithm as nbody.vow.
const PI: f64 = 3.141592653589793;
const SOLAR_MASS: f64 = 4.0 * PI * PI;
const DAYS_PER_YEAR: f64 = 365.24;
const N: usize = 5;

struct System { x: [f64; N], y: [f64; N], z: [f64; N], vx: [f64; N], vy: [f64; N], vz: [f64; N], mass: [f64; N] }

impl System {
    fn energy(&self) -> f64 {
        let mut e = 0.0;
        for i in 0..N {
            e += 0.5 * self.mass[i] * (self.vx[i] * self.vx[i] + self.vy[i] * self.vy[i] + self.vz[i] * self.vz[i]);
            for j in i + 1..N {
                let (dx, dy, dz) = (self.x[i] - self.x[j], self.y[i] - self.y[j], self.z[i] - self.z[j]);
                e -= self.mass[i] * self.mass[j] / (dx * dx + dy * dy + dz * dz).sqrt();
            }
        }
        e
    }

    fn advance(&mut self, dt: f64) {
        for i in 0..N {
            for j in i + 1..N {
                let (dx, dy, dz) = (self.x[i] - self.x[j], self.y[i] - self.y[j], self.z[i] - self.z[j]);
                let d2 = dx * dx + dy * dy + dz * dz;
                let mag = dt / (d2 * d2.sqrt());
                let (mi, mj) = (self.mass[i] * mag, self.mass[j] * mag);
                self.vx[i] -= dx * mj; self.vy[i] -= dy * mj; self.vz[i] -= dz * mj;
                self.vx[j] += dx * mi; self.vy[j] += dy * mi; self.vz[j] += dz * mi;
            }
        }
        for i in 0..N {
            self.x[i] += dt * self.vx[i]; self.y[i] += dt * self.vy[i]; self.z[i] += dt * self.vz[i];
        }
    }
}

fn main() {
    let steps: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let b: [[f64; 7]; N] = [
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        [4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
         1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04],
        [8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
         -2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04],
        [1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
         2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05],
        [1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
         2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05],
    ];
    let mut s = System { x: [0.0; N], y: [0.0; N], z: [0.0; N], vx: [0.0; N], vy: [0.0; N], vz: [0.0; N], mass: [0.0; N] };
    for i in 0..N {
        s.x[i] = b[i][0]; s.y[i] = b[i][1]; s.z[i] = b[i][2];
        s.vx[i] = b[i][3] * DAYS_PER_YEAR; s.vy[i] = b[i][4] * DAYS_PER_YEAR; s.vz[i] = b[i][5] * DAYS_PER_YEAR;
        s.mass[i] = b[i][6] * SOLAR_MASS;
    }
    let (mut px, mut py, mut pz) = (0.0, 0.0, 0.0);
    for i in 0..N {
        px += s.vx[i] * s.mass[i]; py += s.vy[i] * s.mass[i]; pz += s.vz[i] * s.mass[i];
    }
    s.vx[0] = -px / SOLAR_MASS; s.vy[0] = -py / SOLAR_MASS; s.vz[0] = -pz / SOLAR_MASS;
    println!("{:.9}", s.energy());
    for _ in 0..steps {
        s.advance(0.01);
    }
    println!("{:.9}", s.energy());
}
