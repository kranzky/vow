// k-nucleotide: straightforward single-threaded JavaScript with Map.
const fs = require("fs");
const text = fs.readFileSync(process.argv[2], "latin1");
const parts = [];
let inside = false;
for (const line of text.split("\n")) {
  if (line.startsWith(">")) { inside = line.startsWith(">THREE"); continue; }
  if (inside) parts.push(line.toUpperCase());
}
const seq = parts.join("");
function counts(k) {
  const t = new Map();
  for (let i = 0; i + k <= seq.length; i++) {
    const key = seq.substring(i, i + k);
    t.set(key, (t.get(key) || 0) + 1);
  }
  return t;
}
const out = [];
for (const k of [1, 2]) {
  const t = counts(k), total = seq.length - k + 1;
  const e = [...t.entries()].sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
  for (const [key, c] of e) {
    const milli = Math.floor((c * 200000 + total) / (2 * total));
    out.push(`${key} ${Math.floor(milli / 1000)}.${String(milli % 1000).padStart(3, "0")}`);
  }
  out.push("");
}
for (const s of ["GGT", "GGTA", "GGTATT", "GGTATTTTAATT", "GGTATTTTAATTTATAGT"]) {
  out.push(`${counts(s.length).get(s) || 0}\t${s}`);
}
console.log(out.join("\n"));
