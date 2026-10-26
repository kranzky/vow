// Generates the FASTA input for k-nucleotide, following the Benchmarks Game's fasta
// program (linear congruential generator, cumulative probabilities, 60-column lines).
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define IM 139968
#define IA 3877
#define IC 29573
static long last = 42;
static double rnd(double max) { last = (last * IA + IC) % IM; return max * last / IM; }

static const char *alu =
    "GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGGGAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAG"
    "TTCGAGACCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAATACAAAAATTAGCCGGGCGTGGTGGCGCG"
    "CGCCTGTAATCCCAGCTACTCGGGAGGCTGAGGCAGGAGAATCGCTTGAACCCGGGAGGCGGAGGTTGCAGTGAGCC"
    "GAGATCGCGCCACTGCACTCCAGCCTGGGCGACAGAGCGAGACTCCGTCTCAAAAA";

typedef struct { char c; double p; } Freq;
static Freq iub[] = {{'a', 0.27}, {'c', 0.12}, {'g', 0.12}, {'t', 0.27}, {'B', 0.02}, {'D', 0.02},
                     {'H', 0.02}, {'K', 0.02}, {'M', 0.02}, {'N', 0.02}, {'R', 0.02}, {'S', 0.02},
                     {'V', 0.02}, {'W', 0.02}, {'Y', 0.02}};
static Freq homo[] = {{'a', 0.3029549426680}, {'c', 0.1979883004921}, {'g', 0.1975473066391},
                      {'t', 0.3015094502008}};

static void repeat_fasta(const char *s, long n) {
    long len = (long)strlen(s), pos = 0;
    char line[61];
    while (n > 0) {
        long w = n < 60 ? n : 60;
        for (long i = 0; i < w; i++) { line[i] = s[pos++]; if (pos == len) pos = 0; }
        line[w] = 0;
        puts(line);
        n -= w;
    }
}

static void random_fasta(Freq *f, int k, long n) {
    double cum = 0;
    for (int i = 0; i < k; i++) { cum += f[i].p; f[i].p = cum; }
    char line[61];
    while (n > 0) {
        long w = n < 60 ? n : 60;
        for (long i = 0; i < w; i++) {
            double r = rnd(1.0);
            int j = 0;
            while (j < k - 1 && r >= f[j].p) j++;
            line[i] = f[j].c;
        }
        line[w] = 0;
        puts(line);
        n -= w;
    }
}

int main(int argc, char **argv) {
    long n = argc > 1 ? atol(argv[1]) : 1000;
    puts(">ONE Homo sapiens alu");
    repeat_fasta(alu, n * 2);
    puts(">TWO IUB ambiguity codes");
    random_fasta(iub, 15, n * 3);
    puts(">THREE Homo sapiens frequency");
    random_fasta(homo, 4, n * 5);
    return 0;
}
