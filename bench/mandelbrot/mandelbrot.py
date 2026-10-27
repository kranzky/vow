# mandelbrot: the same algorithm as mandelbrot.vow.
import sys

n = int(sys.argv[1]) if len(sys.argv) > 1 else 200
out = bytearray(f"P4\n{n} {n}\n".encode())
for y in range(n):
    ci = 2.0 * y / n - 1.0
    byte = bits = 0
    for x in range(n):
        cr = 2.0 * x / n - 1.5
        zr = zi = tr = ti = 0.0
        i = 0
        while i < 50 and tr + ti <= 4.0:
            zi = 2.0 * zr * zi + ci
            zr = tr - ti + cr
            tr = zr * zr
            ti = zi * zi
            i += 1
        byte = byte * 2 + (1 if tr + ti <= 4.0 else 0)
        bits += 1
        if bits == 8 or x == n - 1:
            byte <<= 8 - bits
            out.append(byte)
            byte = bits = 0
sys.stdout.buffer.write(out)
