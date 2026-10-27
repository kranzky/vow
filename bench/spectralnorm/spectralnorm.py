# spectral-norm: the same algorithm as spectralnorm.vow.
import math
import sys


def eval_a(i, j):
    return 1.0 / ((i + j) * (i + j + 1) // 2 + i + 1)


def times(v, transpose):
    n = len(v)
    out = []
    for i in range(n):
        s = 0.0
        for j in range(n):
            s += (eval_a(j, i) if transpose else eval_a(i, j)) * v[j]
        out.append(s)
    return out


def times_ata(v):
    return times(times(v, False), True)


n = int(sys.argv[1]) if len(sys.argv) > 1 else 100
u = [1.0] * n
v = []
for _ in range(10):
    v = times_ata(u)
    u = times_ata(v)
vbv = vv = 0.0
for i in range(n):
    vbv += u[i] * v[i]
    vv += v[i] * v[i]
print(f"{math.sqrt(vbv / vv):.9f}")
