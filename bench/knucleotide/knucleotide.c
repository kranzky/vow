// k-nucleotide: straightforward single-threaded C with an open-addressing hash table.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>

typedef struct { const char *key; long count; } Entry;
typedef struct { Entry *slots; long cap, size; int k; } Table;

static unsigned long hash(const char *s, int k) {
    unsigned long h = 14695981039346656037ul;
    for (int i = 0; i < k; i++) { h ^= (unsigned char)s[i]; h *= 1099511628211ul; }
    return h;
}

static void put(Table *t, const char *key, long add);

static void grow(Table *t) {
    Entry *old = t->slots;
    long oldcap = t->cap;
    t->cap *= 2;
    t->slots = calloc((size_t)t->cap, sizeof(Entry));
    t->size = 0;
    for (long i = 0; i < oldcap; i++) if (old[i].key) put(t, old[i].key, old[i].count);
    free(old);
}

static void put(Table *t, const char *key, long add) {
    if (2 * (t->size + 1) > t->cap) grow(t);
    unsigned long i = hash(key, t->k) & (unsigned long)(t->cap - 1);
    while (t->slots[i].key && memcmp(t->slots[i].key, key, (size_t)t->k) != 0) i = (i + 1) & (unsigned long)(t->cap - 1);
    if (!t->slots[i].key) { t->slots[i].key = key; t->size++; }
    t->slots[i].count += add;
}

static Table counts(const char *seq, long len, int k) {
    Table t = {calloc(1024, sizeof(Entry)), 1024, 0, k};
    for (long i = 0; i + k <= len; i++) put(&t, seq + i, 1);
    return t;
}

static int k_global;
static int cmp(const void *a, const void *b) {
    const Entry *x = a, *y = b;
    if (x->count != y->count) return x->count > y->count ? -1 : 1;
    return memcmp(x->key, y->key, (size_t)k_global);
}

static void frequencies(const char *seq, long len, int k) {
    Table t = counts(seq, len, k);
    long total = len - k + 1, n = 0;
    Entry *e = malloc(sizeof(Entry) * (size_t)t.size);
    for (long i = 0; i < t.cap; i++) if (t.slots[i].key) e[n++] = t.slots[i];
    k_global = k;
    qsort(e, (size_t)n, sizeof(Entry), cmp);
    for (long i = 0; i < n; i++) {
        long milli = (e[i].count * 200000 + total) / (2 * total);
        printf("%.*s %ld.%03ld\n", k, e[i].key, milli / 1000, milli % 1000);
    }
    printf("\n");
    free(e);
    free(t.slots);
}

int main(int argc, char **argv) {
    FILE *f = fopen(argv[1], "r");
    char line[256];
    size_t cap = 1 << 20, len = 0;
    char *seq = malloc(cap);
    int inside = 0;
    while (fgets(line, sizeof line, f)) {
        if (line[0] == '>') { inside = strncmp(line, ">THREE", 6) == 0; continue; }
        if (!inside) continue;
        for (char *p = line; *p && *p != '\n'; p++) {
            if (len == cap) seq = realloc(seq, cap *= 2);
            seq[len++] = (char)toupper((unsigned char)*p);
        }
    }
    fclose(f);
    frequencies(seq, (long)len, 1);
    frequencies(seq, (long)len, 2);
    const char *wanted[] = {"GGT", "GGTA", "GGTATT", "GGTATTTTAATT", "GGTATTTTAATTTATAGT"};
    for (int w = 0; w < 5; w++) {
        int k = (int)strlen(wanted[w]);
        Table t = counts(seq, (long)len, k);
        long c = 0;
        for (long i = 0; i < t.cap; i++) if (t.slots[i].key && memcmp(t.slots[i].key, wanted[w], (size_t)k) == 0) c = t.slots[i].count;
        printf("%ld\t%s\n", c, wanted[w]);
        free(t.slots);
    }
    return 0;
}
