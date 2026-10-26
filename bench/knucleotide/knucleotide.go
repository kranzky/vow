// k-nucleotide: straightforward single-threaded Go with maps.
package main

import (
	"bufio"
	"fmt"
	"os"
	"sort"
	"strings"
)

func counts(seq string, k int) map[string]int {
	t := map[string]int{}
	for i := 0; i+k <= len(seq); i++ {
		t[seq[i:i+k]]++
	}
	return t
}

func main() {
	f, _ := os.Open(os.Args[1])
	sc := bufio.NewScanner(f)
	var b strings.Builder
	inside := false
	for sc.Scan() {
		line := sc.Text()
		if strings.HasPrefix(line, ">") {
			inside = strings.HasPrefix(line, ">THREE")
			continue
		}
		if inside {
			b.WriteString(strings.ToUpper(line))
		}
	}
	seq := b.String()
	for _, k := range []int{1, 2} {
		t := counts(seq, k)
		total := len(seq) - k + 1
		keys := make([]string, 0, len(t))
		for key := range t {
			keys = append(keys, key)
		}
		sort.Slice(keys, func(i, j int) bool {
			if t[keys[i]] != t[keys[j]] {
				return t[keys[i]] > t[keys[j]]
			}
			return keys[i] < keys[j]
		})
		for _, key := range keys {
			milli := (t[key]*200000 + total) / (2 * total)
			fmt.Printf("%s %d.%03d\n", key, milli/1000, milli%1000)
		}
		fmt.Println()
	}
	for _, s := range []string{"GGT", "GGTA", "GGTATT", "GGTATTTTAATT", "GGTATTTTAATTTATAGT"} {
		fmt.Printf("%d\t%s\n", counts(seq, len(s))[s], s)
	}
}
