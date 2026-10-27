// n-body: the same algorithm as nbody.vow.
package main

import (
	"fmt"
	"math"
	"os"
	"strconv"
)

const (
	pi          = 3.141592653589793
	solarMass   = 4 * pi * pi
	daysPerYear = 365.24
	n           = 5
)

var x, y, z, vx, vy, vz, mass [n]float64

func energy() float64 {
	e := 0.0
	for i := 0; i < n; i++ {
		e += 0.5 * mass[i] * (vx[i]*vx[i] + vy[i]*vy[i] + vz[i]*vz[i])
		for j := i + 1; j < n; j++ {
			dx, dy, dz := x[i]-x[j], y[i]-y[j], z[i]-z[j]
			e -= mass[i] * mass[j] / math.Sqrt(dx*dx+dy*dy+dz*dz)
		}
	}
	return e
}

func advance(dt float64) {
	for i := 0; i < n; i++ {
		for j := i + 1; j < n; j++ {
			dx, dy, dz := x[i]-x[j], y[i]-y[j], z[i]-z[j]
			d2 := dx*dx + dy*dy + dz*dz
			mag := dt / (d2 * math.Sqrt(d2))
			mi, mj := mass[i]*mag, mass[j]*mag
			vx[i] -= dx * mj
			vy[i] -= dy * mj
			vz[i] -= dz * mj
			vx[j] += dx * mi
			vy[j] += dy * mi
			vz[j] += dz * mi
		}
	}
	for i := 0; i < n; i++ {
		x[i] += dt * vx[i]
		y[i] += dt * vy[i]
		z[i] += dt * vz[i]
	}
}

func main() {
	steps := 1000
	if len(os.Args) > 1 {
		steps, _ = strconv.Atoi(os.Args[1])
	}
	b := [n][7]float64{
		{0, 0, 0, 0, 0, 0, 1},
		{4.84143144246472090e+00, -1.16032004402742839e+00, -1.03622044471123109e-01,
			1.66007664274403694e-03, 7.69901118419740425e-03, -6.90460016972063023e-05, 9.54791938424326609e-04},
		{8.34336671824457987e+00, 4.12479856412430479e+00, -4.03523417114321381e-01,
			-2.76742510726862411e-03, 4.99852801234917238e-03, 2.30417297573763929e-05, 2.85885980666130812e-04},
		{1.28943695621391310e+01, -1.51111514016986312e+01, -2.23307578892655734e-01,
			2.96460137564761618e-03, 2.37847173959480950e-03, -2.96589568540237556e-05, 4.36624404335156298e-05},
		{1.53796971148509165e+01, -2.59193146099879641e+01, 1.79258772950371181e-01,
			2.68067772490389322e-03, 1.62824170038242295e-03, -9.51592254519715870e-05, 5.15138902046611451e-05},
	}
	for i := 0; i < n; i++ {
		x[i], y[i], z[i] = b[i][0], b[i][1], b[i][2]
		vx[i], vy[i], vz[i] = b[i][3]*daysPerYear, b[i][4]*daysPerYear, b[i][5]*daysPerYear
		mass[i] = b[i][6] * solarMass
	}
	px, py, pz := 0.0, 0.0, 0.0
	for i := 0; i < n; i++ {
		px += vx[i] * mass[i]
		py += vy[i] * mass[i]
		pz += vz[i] * mass[i]
	}
	vx[0], vy[0], vz[0] = -px/solarMass, -py/solarMass, -pz/solarMass
	fmt.Printf("%.9f\n", energy())
	for k := 0; k < steps; k++ {
		advance(0.01)
	}
	fmt.Printf("%.9f\n", energy())
}
