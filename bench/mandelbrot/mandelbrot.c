// mandelbrot: the same algorithm as mandelbrot.vow.
#include <stdio.h>
#include <stdlib.h>
#pragma STDC FP_CONTRACT OFF

int main(int argc, char **argv) {
    int n = argc > 1 ? atoi(argv[1]) : 200;
    printf("P4\n%d %d\n", n, n);
    for (int y = 0; y < n; y++) {
        double ci = 2.0 * y / n - 1.0;
        int byte = 0, bits = 0;
        for (int x = 0; x < n; x++) {
            double cr = 2.0 * x / n - 1.5;
            double zr = 0, zi = 0, tr = 0, ti = 0;
            for (int i = 0; i < 50 && tr + ti <= 4.0; i++) {
                zi = 2.0 * zr * zi + ci;
                zr = tr - ti + cr;
                tr = zr * zr;
                ti = zi * zi;
            }
            byte = byte * 2 + (tr + ti <= 4.0);
            bits++;
            if (bits == 8 || x == n - 1) {
                byte <<= 8 - bits;
                putchar(byte);
                byte = bits = 0;
            }
        }
    }
    return 0;
}
