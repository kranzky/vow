// fannkuch-redux: straightforward single-threaded JavaScript.
const n = Number(process.argv[2] || 7);
const perm1 = Array.from({ length: n }, (_, i) => i), perm = new Array(n).fill(0), count = new Array(n).fill(0);
let maxFlips = 0, checksum = 0, permCount = 0, r = n;
for (;;) {
  for (; r !== 1; r--) count[r - 1] = r;
  for (let i = 0; i < n; i++) perm[i] = perm1[i];
  let flips = 0;
  for (let k = perm[0]; k !== 0; k = perm[0]) {
    for (let i = 0, j = k; i < j; i++, j--) { const t = perm[i]; perm[i] = perm[j]; perm[j] = t; }
    flips++;
  }
  if (flips > maxFlips) maxFlips = flips;
  checksum += permCount % 2 === 0 ? flips : -flips;
  let done = false;
  for (;;) {
    if (r === n) { done = true; break; }
    const p0 = perm1[0];
    for (let i = 0; i < r; i++) perm1[i] = perm1[i + 1];
    perm1[r] = p0;
    if (--count[r] > 0) break;
    r++;
  }
  if (done) break;
  permCount++;
}
console.log(`${checksum}\nPfannkuchen(${n}) = ${maxFlips}`);
