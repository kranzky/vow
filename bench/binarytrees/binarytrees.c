// binary-trees: straightforward single-threaded C with malloc/free.
#include <stdio.h>
#include <stdlib.h>

typedef struct Node { struct Node *left, *right; } Node;

static Node *make(int depth) {
    Node *n = malloc(sizeof *n);
    if (depth > 0) { n->left = make(depth - 1); n->right = make(depth - 1); }
    else { n->left = n->right = NULL; }
    return n;
}
static long check(const Node *n) { return 1 + (n->left ? check(n->left) + check(n->right) : 0); }
static void release(Node *n) { if (n->left) { release(n->left); release(n->right); } free(n); }

int main(int argc, char **argv) {
    int n = argc > 1 ? atoi(argv[1]) : 10, min_depth = 4;
    int max_depth = n < min_depth + 2 ? min_depth + 2 : n, stretch = max_depth + 1;
    Node *t = make(stretch);
    printf("stretch tree of depth %d\t check: %ld\n", stretch, check(t));
    release(t);
    Node *long_lived = make(max_depth);
    for (int d = min_depth; d <= max_depth; d += 2) {
        long iterations = 1L << (max_depth - d + min_depth), sum = 0;
        for (long i = 0; i < iterations; i++) { t = make(d); sum += check(t); release(t); }
        printf("%ld\t trees of depth %d\t check: %ld\n", iterations, d, sum);
    }
    printf("long lived tree of depth %d\t check: %ld\n", max_depth, check(long_lived));
    return 0;
}
