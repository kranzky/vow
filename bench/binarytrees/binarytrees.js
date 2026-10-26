// binary-trees: straightforward single-threaded JavaScript.
function make(depth) {
  return depth > 0 ? { left: make(depth - 1), right: make(depth - 1) } : { left: null, right: null };
}
function check(n) { return n.left === null ? 1 : 1 + check(n.left) + check(n.right); }

const n = Number(process.argv[2] || 10), minDepth = 4;
const maxDepth = Math.max(minDepth + 2, n), stretch = maxDepth + 1;
const out = [];
out.push(`stretch tree of depth ${stretch}\t check: ${check(make(stretch))}`);
const longLived = make(maxDepth);
for (let d = minDepth; d <= maxDepth; d += 2) {
  const iterations = 2 ** (maxDepth - d + minDepth);
  let sum = 0;
  for (let i = 0; i < iterations; i++) sum += check(make(d));
  out.push(`${iterations}\t trees of depth ${d}\t check: ${sum}`);
}
out.push(`long lived tree of depth ${maxDepth}\t check: ${check(longLived)}`);
console.log(out.join("\n"));
