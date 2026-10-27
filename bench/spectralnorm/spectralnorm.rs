// spectral-norm: the same algorithm as spectralnorm.vow.
fn eval_a(i: i64, j: i64) -> f64 {
    1.0 / ((i + j) * (i + j + 1) / 2 + i + 1) as f64
}

fn times(v: &[f64], transpose: bool) -> Vec<f64> {
    let n = v.len() as i64;
    (0..n)
        .map(|i| {
            let mut sum = 0.0;
            for j in 0..n {
                let a = if transpose { eval_a(j, i) } else { eval_a(i, j) };
                sum += a * v[j as usize];
            }
            sum
        })
        .collect()
}

fn times_ata(v: &[f64]) -> Vec<f64> {
    times(&times(v, false), true)
}

fn main() {
    let n: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let mut u = vec![1.0; n];
    let mut v = vec![0.0; n];
    for _ in 0..10 {
        v = times_ata(&u);
        u = times_ata(&v);
    }
    let (mut vbv, mut vv) = (0.0, 0.0);
    for i in 0..n {
        vbv += u[i] * v[i];
        vv += v[i] * v[i];
    }
    println!("{:.9}", (vbv / vv).sqrt());
}
