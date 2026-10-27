// spectral-norm: the same algorithm as spectralnorm.vow.
package main

import (
	"fmt"
	"math"
	"os"
	"strconv"
)

func evalA(i, j int) float64 { return 1.0 / float64((i+j)*(i+j+1)/2+i+1) }

func times(v []float64, transpose bool) []float64 {
	n := len(v)
	out := make([]float64, n)
	for i := 0; i < n; i++ {
		sum := 0.0
		for j := 0; j < n; j++ {
			a := evalA(i, j)
			if transpose {
				a = evalA(j, i)
			}
			// The conversion stops Go fusing the multiply and the add.
			sum += float64(a * v[j])
		}
		out[i] = sum
	}
	return out
}

func timesAtA(v []float64) []float64 { return times(times(v, false), true) }

func main() {
	n := 100
	if len(os.Args) > 1 {
		n, _ = strconv.Atoi(os.Args[1])
	}
	u := make([]float64, n)
	for i := range u {
		u[i] = 1
	}
	var v []float64
	for k := 0; k < 10; k++ {
		v = timesAtA(u)
		u = timesAtA(v)
	}
	vbv, vv := 0.0, 0.0
	for i := 0; i < n; i++ {
		vbv += float64(u[i] * v[i])
		vv += float64(v[i] * v[i])
	}
	fmt.Printf("%.9f\n", math.Sqrt(vbv/vv))
}
