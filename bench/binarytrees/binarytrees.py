# binary-trees: straightforward single-threaded Python.
import sys

def make(depth):
    return (make(depth - 1), make(depth - 1)) if depth > 0 else (None, None)

def check(node):
    left, right = node
    return 1 if left is None else 1 + check(left) + check(right)

def main():
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 10
    min_depth = 4
    max_depth = max(min_depth + 2, n)
    stretch = max_depth + 1
    print(f"stretch tree of depth {stretch}\t check: {check(make(stretch))}")
    long_lived = make(max_depth)
    for d in range(min_depth, max_depth + 1, 2):
        iterations = 2 ** (max_depth - d + min_depth)
        total = sum(check(make(d)) for _ in range(iterations))
        print(f"{iterations}\t trees of depth {d}\t check: {total}")
    print(f"long lived tree of depth {max_depth}\t check: {check(long_lived)}")

main()
