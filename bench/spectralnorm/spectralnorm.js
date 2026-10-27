// spectral-norm: the same algorithm as spectralnorm.vow.
function evalA(i, j) { return 1 / ((i + j) * (i + j + 1) / 2 + i + 1); }

function times(v, transpose) {
  const n = v.length, out = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    let sum = 0;
    for (let j = 0; j < n; j++) sum += (transpose ? evalA(j, i) : evalA(i, j)) * v[j];
    out[i] = sum;
  }
  return out;
}

function timesAtA(v) { return times(times(v, false), true); }

const n = process.argv.length > 2 ? parseInt(process.argv[2]) : 100;
let u = new Float64Array(n).fill(1), v = new Float64Array(n);
for (let k = 0; k < 10; k++) {
  v = timesAtA(u);
  u = timesAtA(v);
}
let vbv = 0, vv = 0;
for (let i = 0; i < n; i++) {
  vbv += u[i] * v[i];
  vv += v[i] * v[i];
}
console.log(Math.sqrt(vbv / vv).toFixed(9));
