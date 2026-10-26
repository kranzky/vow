// Vow runtime, v0 (frozen). The Rust bootstrap compiler emits C for this runtime: no
// reference counting, memory is never freed. The self-hosted compiler uses
// runtime/vow_rt.h, which is reference counted.
//
// Every Vow value is one 64-bit word (V). Integers and booleans are
// unboxed; everything else is a pointer to an immutable heap object. Memory comes
// from a bump allocator and is never freed.
//
// Define VOW_VERIFIED before including this header when every proof obligation was
// discharged; bounds and divisor checks then compile to nothing. Overflow checks always
// stay, because the verifier models Int as mathematical integers and relies on
// overflow trapping.
#ifndef VOW_RT_H
#define VOW_RT_H

#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <dirent.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

typedef uint64_t V;

#define VI(x) ((int64_t)(x))
#define VP(x) ((V *)(uintptr_t)(x))

static __attribute__((noreturn)) void vow_panic(const char *msg) {
    fflush(stdout);
    fprintf(stderr, "vow: panic: %s\n", msg);
    exit(101);
}

#ifdef VOW_VERIFIED
#define VOW_REQUIRE(cond, msg) ((void)0)
#else
#define VOW_REQUIRE(cond, msg) \
    do {                       \
        if (!(cond)) vow_panic(msg); \
    } while (0)
#endif

// ---- allocation ----

static char *vow_heap_ptr = 0;
static size_t vow_heap_left = 0;

static void *vow_alloc(size_t n) {
    n = (n + 15) & ~(size_t)15;
    if (n > vow_heap_left) {
        size_t chunk = n > (64u << 20) ? n : (64u << 20);
        vow_heap_ptr = (char *)malloc(chunk);
        if (!vow_heap_ptr) vow_panic("out of memory");
        vow_heap_left = chunk;
    }
    void *p = vow_heap_ptr;
    vow_heap_ptr += n;
    vow_heap_left -= n;
    return p;
}

static V vow_new(int64_t words) {
    return (V)(uintptr_t)vow_alloc((size_t)(words > 0 ? words : 1) * sizeof(V));
}

static V vow_copy(V obj, int64_t words) {
    V r = vow_new(words);
    memcpy(VP(r), VP(obj), (size_t)words * sizeof(V));
    return r;
}

// ---- integers ----

static inline V vadd(V a, V b) {
    int64_t r;
    if (__builtin_add_overflow(VI(a), VI(b), &r)) vow_panic("integer overflow in '+'");
    return (V)r;
}
static inline V vsub(V a, V b) {
    int64_t r;
    if (__builtin_sub_overflow(VI(a), VI(b), &r)) vow_panic("integer overflow in '-'");
    return (V)r;
}
static inline V vmul(V a, V b) {
    int64_t r;
    if (__builtin_mul_overflow(VI(a), VI(b), &r)) vow_panic("integer overflow in '*'");
    return (V)r;
}
static inline V vneg(V a) {
    if (VI(a) == INT64_MIN) vow_panic("integer overflow in unary '-'");
    return (V)(-VI(a));
}
// Euclidean division: the remainder is always non-negative (matches SMT-LIB).
static inline V vdiv(V a, V b) {
    VOW_REQUIRE(VI(b) != 0, "division by zero");
    if (VI(a) == INT64_MIN && VI(b) == -1) vow_panic("integer overflow in '/'");
    int64_t q = VI(a) / VI(b), r = VI(a) % VI(b);
    if (r < 0) q = VI(b) > 0 ? q - 1 : q + 1;
    return (V)q;
}
static inline V vmod(V a, V b) {
    VOW_REQUIRE(VI(b) != 0, "division by zero");
    if (VI(b) == -1) return 0;
    int64_t r = VI(a) % VI(b);
    if (r < 0) r = VI(b) > 0 ? r + VI(b) : r - VI(b);
    return (V)r;
}

// ---- strings ----

typedef struct {
    int64_t len;
    char data[];
} VStr;

#define VS(x) ((VStr *)(uintptr_t)(x))

static V vstr_new(const char *p, int64_t n) {
    VStr *s = (VStr *)vow_alloc(sizeof(VStr) + (size_t)n + 1);
    s->len = n;
    if (n) memcpy(s->data, p, (size_t)n);
    s->data[n] = 0;
    return (V)(uintptr_t)s;
}

static inline V vstr_len(V s) { return (V)VS(s)->len; }

static V vstr_cat(V a, V b) {
    int64_t n = VS(a)->len + VS(b)->len;
    VStr *s = (VStr *)vow_alloc(sizeof(VStr) + (size_t)n + 1);
    s->len = n;
    memcpy(s->data, VS(a)->data, (size_t)VS(a)->len);
    memcpy(s->data + VS(a)->len, VS(b)->data, (size_t)VS(b)->len);
    s->data[n] = 0;
    return (V)(uintptr_t)s;
}

static inline V vstr_eq(V a, V b) {
    return (V)(VS(a)->len == VS(b)->len && memcmp(VS(a)->data, VS(b)->data, (size_t)VS(a)->len) == 0);
}

static int vstr_cmp(V a, V b) {
    int64_t n = VS(a)->len < VS(b)->len ? VS(a)->len : VS(b)->len;
    int c = memcmp(VS(a)->data, VS(b)->data, (size_t)n);
    if (c) return c;
    return VS(a)->len < VS(b)->len ? -1 : (VS(a)->len > VS(b)->len ? 1 : 0);
}

static inline V vstr_byte(V s, V i) {
    VOW_REQUIRE(VI(i) >= 0 && VI(i) < VS(s)->len, "string index out of bounds");
    return (V)(unsigned char)VS(s)->data[VI(i)];
}

static V vstr_slice(V s, V from, V to) {
    VOW_REQUIRE(0 <= VI(from) && VI(from) <= VI(to) && VI(to) <= VS(s)->len, "string slice out of bounds");
    return vstr_new(VS(s)->data + VI(from), VI(to) - VI(from));
}

static V vchar(V code) {
    VOW_REQUIRE(VI(code) >= 0 && VI(code) < 256, "char code out of range");
    char c = (char)VI(code);
    return vstr_new(&c, 1);
}

static V vint_to_str(V n) {
    char buf[32];
    int k = snprintf(buf, sizeof buf, "%lld", (long long)VI(n));
    return vstr_new(buf, k);
}

static V vhash(V s) {
    uint64_t h = 14695981039346656037ull;
    for (int64_t i = 0; i < VS(s)->len; i++) {
        h ^= (unsigned char)VS(s)->data[i];
        h *= 1099511628211ull;
    }
    char buf[17];
    snprintf(buf, sizeof buf, "%016llx", (unsigned long long)h);
    return vstr_new(buf, 16);
}

// ---- enums and options ----

static V vow_variant0(int64_t tag) {
    V r = vow_new(1);
    VP(r)[0] = (V)tag;
    return r;
}

static V vsome(V x) {
    V r = vow_new(2);
    VP(r)[0] = 1;
    VP(r)[1] = x;
    return r;
}

static V vnone(void) { return vow_variant0(0); }

static V vstr_to_int(V s) {
    int64_t n = VS(s)->len, i = 0, v = 0;
    int neg = 0;
    if (n == 0) return vnone();
    if (VS(s)->data[0] == '-') {
        neg = 1;
        i = 1;
        if (n == 1) return vnone();
    }
    for (; i < n; i++) {
        char c = VS(s)->data[i];
        if (c < '0' || c > '9') return vnone();
        if (__builtin_mul_overflow(v, 10, &v)) return vnone();
        if (__builtin_add_overflow(v, (int64_t)(neg ? -(c - '0') : (c - '0')), &v)) return vnone();
    }
    return vsome((V)v);
}

// ---- lists ----
// A list is (len, buf). Pushing onto a list whose length equals buf->used appends in
// place; any older, shorter list sharing the buffer never sees the new slot, so the
// value semantics are preserved and repeated pushes are amortised O(1).

typedef struct {
    int64_t cap;
    int64_t used;
    V items[];
} VBuf;

typedef struct {
    int64_t len;
    VBuf *buf;
} VList;

#define VL(x) ((VList *)(uintptr_t)(x))

static VBuf *vbuf_new(int64_t cap) {
    if (cap < 4) cap = 4;
    VBuf *b = (VBuf *)vow_alloc(sizeof(VBuf) + (size_t)cap * sizeof(V));
    b->cap = cap;
    b->used = 0;
    return b;
}

static V vlist_empty(void) {
    VList *l = (VList *)vow_alloc(sizeof(VList));
    l->len = 0;
    l->buf = 0;
    return (V)(uintptr_t)l;
}

static inline V vlist_len(V l) { return (V)VL(l)->len; }

static V vlist_push(V lv, V x) {
    VList *l = VL(lv);
    VBuf *b = l->buf;
    if (!b || b->used != l->len || b->used == b->cap) {
        VBuf *nb = vbuf_new(l->len * 2);
        if (l->len) memcpy(nb->items, b->items, (size_t)l->len * sizeof(V));
        nb->used = l->len;
        b = nb;
    }
    b->items[b->used++] = x;
    VList *r = (VList *)vow_alloc(sizeof(VList));
    r->len = l->len + 1;
    r->buf = b;
    return (V)(uintptr_t)r;
}

static inline V vlist_get(V l, V i) {
    VOW_REQUIRE(VI(i) >= 0 && VI(i) < VL(l)->len, "list index out of bounds");
    return VL(l)->buf->items[VI(i)];
}

static V vlist_slice(V lv, V from, V to) {
    VOW_REQUIRE(0 <= VI(from) && VI(from) <= VI(to) && VI(to) <= VL(lv)->len, "list slice out of bounds");
    int64_t n = VI(to) - VI(from);
    VBuf *b = vbuf_new(n);
    if (n) memcpy(b->items, VL(lv)->buf->items + VI(from), (size_t)n * sizeof(V));
    b->used = n;
    VList *r = (VList *)vow_alloc(sizeof(VList));
    r->len = n;
    r->buf = b;
    return (V)(uintptr_t)r;
}

static V vlist_cat(V a, V b) {
    V r = a;
    for (int64_t i = 0; i < VL(b)->len; i++) r = vlist_push(r, VL(b)->buf->items[i]);
    return r;
}

// set_at(list, i, v): a new list with element i replaced (copying, never in place).
static V vlist_set_at(V lv, V i, V x) {
    VOW_REQUIRE(VI(i) >= 0 && VI(i) < VL(lv)->len, "set_at index out of bounds");
    V r = vlist_slice(lv, 0, (V)VL(lv)->len);
    VL(r)->buf->items[VI(i)] = x;
    return r;
}

static V vjoin(V parts, V sep) {
    int64_t n = 0, k = VL(parts)->len;
    for (int64_t i = 0; i < k; i++) n += VS(VL(parts)->buf->items[i])->len;
    if (k > 1) n += (k - 1) * VS(sep)->len;
    VStr *s = (VStr *)vow_alloc(sizeof(VStr) + (size_t)n + 1);
    s->len = n;
    char *p = s->data;
    for (int64_t i = 0; i < k; i++) {
        if (i) {
            memcpy(p, VS(sep)->data, (size_t)VS(sep)->len);
            p += VS(sep)->len;
        }
        VStr *x = VS(VL(parts)->buf->items[i]);
        memcpy(p, x->data, (size_t)x->len);
        p += x->len;
    }
    *p = 0;
    return (V)(uintptr_t)s;
}

// ---- maps: persistent AVL trees with path copying ----

typedef struct VNode {
    V key, val;
    struct VNode *l, *r;
    int64_t height, size;
} VNode;

static inline int64_t vn_h(VNode *n) { return n ? n->height : 0; }
static inline int64_t vn_s(VNode *n) { return n ? n->size : 0; }

static int vkey_cmp(V a, V b, int str) {
    if (str) return vstr_cmp(a, b);
    return VI(a) < VI(b) ? -1 : (VI(a) > VI(b) ? 1 : 0);
}

static VNode *vn_make(V k, V v, VNode *l, VNode *r) {
    VNode *n = (VNode *)vow_alloc(sizeof(VNode));
    n->key = k;
    n->val = v;
    n->l = l;
    n->r = r;
    int64_t hl = vn_h(l), hr = vn_h(r);
    n->height = (hl > hr ? hl : hr) + 1;
    n->size = vn_s(l) + vn_s(r) + 1;
    return n;
}

static VNode *vn_balance(V k, V v, VNode *l, VNode *r) {
    int64_t hl = vn_h(l), hr = vn_h(r);
    if (hl > hr + 1) {
        if (vn_h(l->l) >= vn_h(l->r)) return vn_make(l->key, l->val, l->l, vn_make(k, v, l->r, r));
        return vn_make(l->r->key, l->r->val, vn_make(l->key, l->val, l->l, l->r->l), vn_make(k, v, l->r->r, r));
    }
    if (hr > hl + 1) {
        if (vn_h(r->r) >= vn_h(r->l)) return vn_make(r->key, r->val, vn_make(k, v, l, r->l), r->r);
        return vn_make(r->l->key, r->l->val, vn_make(k, v, l, r->l->l), vn_make(r->key, r->val, r->l->r, r->r));
    }
    return vn_make(k, v, l, r);
}

static VNode *vn_insert(VNode *n, V k, V v, int str) {
    if (!n) return vn_make(k, v, 0, 0);
    int c = vkey_cmp(k, n->key, str);
    if (c == 0) return vn_make(k, v, n->l, n->r);
    if (c < 0) return vn_balance(n->key, n->val, vn_insert(n->l, k, v, str), n->r);
    return vn_balance(n->key, n->val, n->l, vn_insert(n->r, k, v, str));
}

static V vmap_empty(void) { return 0; }
static inline V vmap_len(V m) { return (V)vn_s((VNode *)(uintptr_t)m); }

static V vmap_insert(V m, V k, V v, int str) {
    return (V)(uintptr_t)vn_insert((VNode *)(uintptr_t)m, k, v, str);
}

static VNode *vn_find(V m, V k, int str) {
    VNode *n = (VNode *)(uintptr_t)m;
    while (n) {
        int c = vkey_cmp(k, n->key, str);
        if (c == 0) return n;
        n = c < 0 ? n->l : n->r;
    }
    return 0;
}

static V vmap_lookup(V m, V k, int str) {
    VNode *n = vn_find(m, k, str);
    return n ? vsome(n->val) : vnone();
}

static V vmap_has(V m, V k, int str) { return (V)(vn_find(m, k, str) != 0); }

static V vn_keys(VNode *n, V acc) {
    if (!n) return acc;
    acc = vn_keys(n->l, acc);
    acc = vlist_push(acc, n->key);
    return vn_keys(n->r, acc);
}

static V vmap_keys(V m) { return vn_keys((VNode *)(uintptr_t)m, vlist_empty()); }

// ---- effects ----

static int vow_argc;
static char **vow_argv;

static V vprint(V s) {
    fwrite(VS(s)->data, 1, (size_t)VS(s)->len, stdout);
    return 0;
}

static V veprint(V s) {
    fflush(stdout);
    fwrite(VS(s)->data, 1, (size_t)VS(s)->len, stderr);
    return 0;
}

static V vread_file(V path) {
    FILE *f = fopen(VS(path)->data, "rb");
    if (!f) return vnone();
    fseek(f, 0, SEEK_END);
    long n = ftell(f);
    fseek(f, 0, SEEK_SET);
    VStr *s = (VStr *)vow_alloc(sizeof(VStr) + (size_t)n + 1);
    s->len = (int64_t)fread(s->data, 1, (size_t)n, f);
    s->data[s->len] = 0;
    fclose(f);
    return vsome((V)(uintptr_t)s);
}

static V vwrite_file(V path, V contents) {
    FILE *f = fopen(VS(path)->data, "wb");
    if (!f) return 0;
    size_t w = fwrite(VS(contents)->data, 1, (size_t)VS(contents)->len, f);
    int ok = fclose(f) == 0 && w == (size_t)VS(contents)->len;
    return (V)ok;
}

// make_dir(path): create the directory and any missing parents, like `mkdir -p`.
static V vmake_dir(V path) {
    int64_t n = VS(path)->len;
    if (n == 0) return 0;
    char *p = (char *)malloc((size_t)n + 1);
    memcpy(p, VS(path)->data, (size_t)n + 1);
    for (int64_t i = 1; i < n; i++) {
        if (p[i] == '/') {
            p[i] = 0;
            mkdir(p, 0777);
            p[i] = '/';
        }
    }
    mkdir(p, 0777);
    struct stat st;
    int ok = stat(p, &st) == 0 && S_ISDIR(st.st_mode);
    free(p);
    return (V)ok;
}

static int vow_name_cmp(const void *a, const void *b) {
    return vstr_cmp(*(const V *)a, *(const V *)b);
}

// list_dir(path): the names in a directory, sorted, without "." and "..".
static V vlist_dir(V path) {
    DIR *d = opendir(VS(path)->data);
    if (!d) return vnone();
    int64_t cap = 16, n = 0;
    V *names = (V *)malloc(sizeof(V) * (size_t)cap);
    struct dirent *e;
    while ((e = readdir(d)) != 0) {
        if (strcmp(e->d_name, ".") == 0 || strcmp(e->d_name, "..") == 0) continue;
        if (n == cap) names = (V *)realloc(names, sizeof(V) * (size_t)(cap *= 2));
        names[n++] = vstr_new(e->d_name, (int64_t)strlen(e->d_name));
    }
    closedir(d);
    qsort(names, (size_t)n, sizeof(V), vow_name_cmp);
    V l = vlist_empty();
    for (int64_t i = 0; i < n; i++) l = vlist_push(l, names[i]);
    free(names);
    return vsome(l);
}

static V vargs(void) {
    V l = vlist_empty();
    for (int i = 1; i < vow_argc; i++) l = vlist_push(l, vstr_new(vow_argv[i], (int64_t)strlen(vow_argv[i])));
    return l;
}

static V vgetenv(V name) {
    const char *v = getenv(VS(name)->data);
    if (!v) return vnone();
    return vsome(vstr_new(v, (int64_t)strlen(v)));
}

static __attribute__((noreturn)) void vexit(V code) {
    fflush(stdout);
    exit((int)VI(code));
}

// run(argv, input): stdin comes from a temporary file so that large inputs and outputs
// cannot deadlock on full pipes; stdout is captured, stderr is inherited.
static V vrun(V argv, V input) {
    int64_t n = VL(argv)->len;
    V out = vow_new(2);
    VP(out)[0] = (V)(int64_t)-1;
    VP(out)[1] = vstr_new("", 0);
    if (n == 0) return out;
    char tmpl[] = "/tmp/vow-run-XXXXXX";
    int in_fd = mkstemp(tmpl);
    if (in_fd < 0) return out;
    unlink(tmpl);
    if (write(in_fd, VS(input)->data, (size_t)VS(input)->len) != (ssize_t)VS(input)->len) {
        close(in_fd);
        return out;
    }
    lseek(in_fd, 0, SEEK_SET);
    int pfd[2];
    if (pipe(pfd) != 0) {
        close(in_fd);
        return out;
    }
    fflush(stdout);
    pid_t pid = fork();
    if (pid == 0) {
        dup2(in_fd, 0);
        dup2(pfd[1], 1);
        close(pfd[0]);
        close(pfd[1]);
        close(in_fd);
        char **av = (char **)malloc(sizeof(char *) * (size_t)(n + 1));
        for (int64_t i = 0; i < n; i++) av[i] = VS(VL(argv)->buf->items[i])->data;
        av[n] = 0;
        execvp(av[0], av);
        _exit(127);
    }
    close(pfd[1]);
    close(in_fd);
    size_t cap = 4096, len = 0;
    char *buf = (char *)malloc(cap);
    for (;;) {
        if (len == cap) buf = (char *)realloc(buf, cap *= 2);
        ssize_t k = read(pfd[0], buf + len, cap - len);
        if (k <= 0) break;
        len += (size_t)k;
    }
    close(pfd[0]);
    int status = 0;
    waitpid(pid, &status, 0);
    VP(out)[0] = (V)(int64_t)(WIFEXITED(status) ? WEXITSTATUS(status) : -1);
    VP(out)[1] = vstr_new(buf, (int64_t)len);
    free(buf);
    return out;
}

#endif
