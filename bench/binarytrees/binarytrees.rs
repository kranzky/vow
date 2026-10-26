// binary-trees: straightforward single-threaded Rust with Box.
struct Node { children: Option<(Box<Node>, Box<Node>)> }

fn make(depth: u32) -> Box<Node> {
    if depth > 0 { Box::new(Node { children: Some((make(depth - 1), make(depth - 1))) }) }
    else { Box::new(Node { children: None }) }
}
fn check(n: &Node) -> u64 {
    match &n.children { Some((l, r)) => 1 + check(l) + check(r), None => 1 }
}

fn main() {
    let n: u32 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10);
    let min_depth = 4;
    let max_depth = if n < min_depth + 2 { min_depth + 2 } else { n };
    let stretch = max_depth + 1;
    println!("stretch tree of depth {}\t check: {}", stretch, check(&make(stretch)));
    let long_lived = make(max_depth);
    let mut d = min_depth;
    while d <= max_depth {
        let iterations: u64 = 1 << (max_depth - d + min_depth);
        let mut sum = 0;
        for _ in 0..iterations { sum += check(&make(d)); }
        println!("{}\t trees of depth {}\t check: {}", iterations, d, sum);
        d += 2;
    }
    println!("long lived tree of depth {}\t check: {}", max_depth, check(&long_lived));
}
