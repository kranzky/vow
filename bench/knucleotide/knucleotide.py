# k-nucleotide: straightforward single-threaded Python with dict.
import sys

def counts(seq, k):
    table = {}
    for i in range(len(seq) - k + 1):
        key = seq[i:i + k]
        table[key] = table.get(key, 0) + 1
    return table

def main():
    parts, inside = [], False
    with open(sys.argv[1]) as f:
        for line in f:
            if line.startswith(">"):
                inside = line.startswith(">THREE")
            elif inside:
                parts.append(line.rstrip("\n").upper())
    seq = "".join(parts)
    out = []
    for k in (1, 2):
        table, total = counts(seq, k), len(seq) - k + 1
        for key, c in sorted(table.items(), key=lambda kv: (-kv[1], kv[0])):
            milli = (c * 200000 + total) // (2 * total)
            out.append(f"{key} {milli // 1000}.{milli % 1000:03d}")
        out.append("")
    for s in ("GGT", "GGTA", "GGTATT", "GGTATTTTAATT", "GGTATTTTAATTTATAGT"):
        out.append(f"{counts(seq, len(s)).get(s, 0)}\t{s}")
    print("\n".join(out))

main()
