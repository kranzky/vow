// k-nucleotide: straightforward single-threaded Rust with HashMap.
use std::collections::HashMap;

fn counts(seq: &[u8], k: usize) -> HashMap<&[u8], u64> {
    let mut t = HashMap::new();
    for i in 0..=seq.len().saturating_sub(k) {
        if i + k <= seq.len() { *t.entry(&seq[i..i + k]).or_insert(0) += 1; }
    }
    t
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: knucleotide <fasta>");
    let text = std::fs::read_to_string(path).unwrap();
    let mut seq = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with('>') { inside = line.starts_with(">THREE"); continue; }
        if inside { seq.extend(line.bytes().map(|b| b.to_ascii_uppercase())); }
    }
    for k in [1, 2] {
        let t = counts(&seq, k);
        let total = (seq.len() - k + 1) as u64;
        let mut e: Vec<_> = t.into_iter().collect();
        e.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        for (key, c) in e {
            let milli = (c * 200000 + total) / (2 * total);
            println!("{} {}.{:03}", std::str::from_utf8(key).unwrap(), milli / 1000, milli % 1000);
        }
        println!();
    }
    for s in ["GGT", "GGTA", "GGTATT", "GGTATTTTAATT", "GGTATTTTAATTTATAGT"] {
        let t = counts(&seq, s.len());
        println!("{}\t{}", t.get(s.as_bytes()).copied().unwrap_or(0), s);
    }
}
