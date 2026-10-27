// mandelbrot: the same algorithm as mandelbrot.vow.
package main

import (
	"bufio"
	"fmt"
	"os"
	"strconv"
)

func main() {
	n := 200
	if len(os.Args) > 1 {
		n, _ = strconv.Atoi(os.Args[1])
	}
	w := bufio.NewWriter(os.Stdout)
	defer w.Flush()
	fmt.Fprintf(w, "P4\n%d %d\n", n, n)
	size := float64(n)
	for y := 0; y < n; y++ {
		ci := 2.0*float64(y)/size - 1.0
		b, bits := 0, 0
		for x := 0; x < n; x++ {
			cr := 2.0*float64(x)/size - 1.5
			zr, zi, tr, ti := 0.0, 0.0, 0.0, 0.0
			for i := 0; i < 50 && tr+ti <= 4.0; i++ {
				// The conversions stop Go fusing a multiply and an add.
				zi = float64(2.0*zr*zi) + ci
				zr = tr - ti + cr
				tr = zr * zr
				ti = zi * zi
			}
			b *= 2
			if tr+ti <= 4.0 {
				b++
			}
			bits++
			if bits == 8 || x == n-1 {
				b <<= 8 - bits
				w.WriteByte(byte(b))
				b, bits = 0, 0
			}
		}
	}
}
