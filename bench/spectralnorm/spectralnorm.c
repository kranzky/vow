// spectral-norm: the same algorithm as spectralnorm.vow.
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#pragma STDC FP_CONTRACT OFF

static double eval_a(long i, long j) { return 1.0 / (double)((i + j) * (i + j + 1) / 2 + i + 1); }

static void times(const double *v, double *out, long n, int transpose) {
    for (long i = 0; i < n; i++) {
        double sum = 0;
        for (long j = 0; j < n; j++) sum += (transpose ? eval_a(j, i) : eval_a(i, j)) * v[j];
        out[i] = sum;
    }
}

static void times_ata(const double *v, double *out, double *tmp, long n) {
    times(v, tmp, n, 0);
    times(tmp, out, n, 1);
}

int main(int argc, char **argv) {
    long n = argc > 1 ? atol(argv[1]) : 100;
    double *u = malloc(n * sizeof *u), *v = malloc(n * sizeof *v), *tmp = malloc(n * sizeof *tmp);
    for (long i = 0; i < n; i++) u[i] = 1;
    for (int k = 0; k < 10; k++) {
        times_ata(u, v, tmp, n);
        times_ata(v, u, tmp, n);
    }
    double vbv = 0, vv = 0;
    for (long i = 0; i < n; i++) {
        vbv += u[i] * v[i];
        vv += v[i] * v[i];
    }
    printf("%.9f\n", sqrt(vbv / vv));
    return 0;
}
