// mandelbrot: the same algorithm as mandelbrot.vow.
use std::io::Write;

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(200);
    let size = n as f64;
    let mut out: Vec<u8> = format!("P4\n{} {}\n", n, n).into_bytes();
    for y in 0..n {
        let ci = 2.0 * y as f64 / size - 1.0;
        let (mut byte, mut bits) = (0u32, 0u32);
        for x in 0..n {
            let cr = 2.0 * x as f64 / size - 1.5;
            let (mut zr, mut zi, mut tr, mut ti) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
            let mut i = 0;
            while i < 50 && tr + ti <= 4.0 {
                zi = 2.0 * zr * zi + ci;
                zr = tr - ti + cr;
                tr = zr * zr;
                ti = zi * zi;
                i += 1;
            }
            byte = byte * 2 + (tr + ti <= 4.0) as u32;
            bits += 1;
            if bits == 8 || x == n - 1 {
                byte <<= 8 - bits;
                out.push(byte as u8);
                byte = 0;
                bits = 0;
            }
        }
    }
    std::io::stdout().write_all(&out).unwrap();
}
