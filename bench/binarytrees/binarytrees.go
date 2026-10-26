// binary-trees: straightforward single-threaded Go.
package main

import (
	"fmt"
	"os"
	"strconv"
)

type Node struct{ left, right *Node }

func make_(depth int) *Node {
	if depth > 0 {
		return &Node{make_(depth - 1), make_(depth - 1)}
	}
	return &Node{}
}

func check(n *Node) int {
	if n.left == nil {
		return 1
	}
	return 1 + check(n.left) + check(n.right)
}

func main() {
	n := 10
	if len(os.Args) > 1 {
		n, _ = strconv.Atoi(os.Args[1])
	}
	minDepth := 4
	maxDepth := n
	if maxDepth < minDepth+2 {
		maxDepth = minDepth + 2
	}
	stretch := maxDepth + 1
	fmt.Printf("stretch tree of depth %d\t check: %d\n", stretch, check(make_(stretch)))
	longLived := make_(maxDepth)
	for d := minDepth; d <= maxDepth; d += 2 {
		iterations := 1 << (maxDepth - d + minDepth)
		sum := 0
		for i := 0; i < iterations; i++ {
			sum += check(make_(d))
		}
		fmt.Printf("%d\t trees of depth %d\t check: %d\n", iterations, d, sum)
	}
	fmt.Printf("long lived tree of depth %d\t check: %d\n", maxDepth, check(longLived))
}
