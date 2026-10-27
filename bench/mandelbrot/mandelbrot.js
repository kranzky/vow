// mandelbrot: the same algorithm as mandelbrot.vow.
const n = process.argv.length > 2 ? parseInt(process.argv[2]) : 200;
const header = Buffer.from(`P4\n${n} ${n}\n`);
const rowBytes = Math.ceil(n / 8);
const out = Buffer.alloc(rowBytes * n);
let pos = 0;
for (let y = 0; y < n; y++) {
  const ci = 2.0 * y / n - 1.0;
  let byte = 0, bits = 0;
  for (let x = 0; x < n; x++) {
    const cr = 2.0 * x / n - 1.5;
    let zr = 0, zi = 0, tr = 0, ti = 0;
    for (let i = 0; i < 50 && tr + ti <= 4.0; i++) {
      zi = 2.0 * zr * zi + ci;
      zr = tr - ti + cr;
      tr = zr * zr;
      ti = zi * zi;
    }
    byte = byte * 2 + (tr + ti <= 4.0 ? 1 : 0);
    bits++;
    if (bits === 8 || x === n - 1) {
      byte <<= 8 - bits;
      out[pos++] = byte;
      byte = 0;
      bits = 0;
    }
  }
}
process.stdout.write(Buffer.concat([header, out]));
