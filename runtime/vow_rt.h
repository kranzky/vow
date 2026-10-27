// Vow runtime, reference counted.
//
// Every Vow value is one 64-bit word (V). Integers and booleans are unboxed; everything
// else is a pointer to a heap object, preceded by a 16-byte header holding its
// reference count and layout. Objects are freed when their count reaches zero, and an
// object with a count of one is updated in place by `with`, `push`, `set_at` and
// `insert`, which preserves value semantics because nothing else can see it.
//
// Ownership conventions (the code generator relies on these):
// - A function "consumes" an argument when it takes over the caller's reference, and
//   "borrows" it otherwise. Results are always owned by the caller.
// - Consumed: vlist_push(list, item), vlist_set_at(list, _, value), vlist_cat(list, _),
//   vmap_insert(map, key, value), vow_with(base), vsome(value),
//   vvalue_or(_, default).
// - Everything else borrows its arguments.
// - vlist_get and field reads return borrowed references.
//
// Define VOW_VERIFIED when every proof obligation was discharged: bounds and divisor
// checks then compile to nothing. Overflow checks always stay. Define VOW_RC_DEBUG to
// keep freed memory poisoned (never reused) and panic on any use after free; with
// VOW_LEAKS=1 in the environment, the program reports objects still live at exit.
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

// Generated code carries #line directives, so __FILE__ and __LINE__ where it calls a
// checked operation name the .vow source line. Each checked operation is a macro that
// passes them on (vadd(a, b) is vadd_at(a, b, __FILE__, __LINE__)); they are used only
// when the check fails.
static __attribute__((noreturn, cold)) void vow_panic_at(const char *file, int line, const char *msg) {
    fflush(stdout);
    fprintf(stderr, "vow: panic at %s:%d: %s\n", file, line, msg);
    exit(101);
}

#define VOW_PANIC(msg) vow_panic_at(__FILE__, __LINE__, (msg))
#define VOW_AT const char *vow_f, int vow_l

#ifdef VOW_VERIFIED
#define VOW_REQUIRE(cond, msg) ((void)0)
#else
#define VOW_REQUIRE(cond, msg) \
    do {                       \
        if (!(cond)) vow_panic_at(vow_f, vow_l, msg); \
    } while (0)
#endif

// ---- objects ----

enum { VK_FIELDS, VK_STR, VK_LIST, VK_BUF, VK_NODE, VK_HASH };

typedef struct {
    int32_t rc;     // reference count; -1 for immortal objects (literals, None)
    uint8_t kind;   // VK_*
    uint8_t flags;  // lists and buffers: elements are heap values; nodes: see below
    uint16_t words; // VK_FIELDS: number of payload words
    uint64_t mask;  // VK_FIELDS: bit i set when payload word i is a heap value
} VHdr;

#define VH(x) ((VHdr *)((char *)(uintptr_t)(x) - sizeof(VHdr)))

// ---- allocation: size classes of 16 bytes with free lists, carved from chunks ----

#define VOW_CLASSES 64
static void *vow_freelist[VOW_CLASSES + 1];
static char *vow_chunk;
static size_t vow_chunk_left;
static int64_t vow_live, vow_immortals;

static void *vow_raw_alloc(size_t n) {
    size_t c = n / 16;
    if (c <= VOW_CLASSES) {
        void *p = vow_freelist[c];
        if (p) {
            vow_freelist[c] = *(void **)p;
            return p;
        }
        if (n > vow_chunk_left) {
            vow_chunk_left = (size_t)1 << 20;
            vow_chunk = (char *)malloc(vow_chunk_left);
            if (!vow_chunk) vow_panic("out of memory");
        }
        p = vow_chunk;
        vow_chunk += n;
        vow_chunk_left -= n;
        return p;
    }
    void *p = malloc(n);
    if (!p) vow_panic("out of memory");
    return p;
}

static void vow_raw_free(void *p, size_t n) {
#ifdef VOW_RC_DEBUG
    VHdr *h = (VHdr *)p;
    memset((char *)p + sizeof(VHdr), 0xdd, n - sizeof(VHdr));
    h->rc = 0; // a freed object: any later use panics
#else
    size_t c = n / 16;
    if (c <= VOW_CLASSES) {
        *(void **)p = vow_freelist[c];
        vow_freelist[c] = p;
    } else {
        free(p);
    }
#endif
}

static inline size_t vow_round(size_t n) { return (n + 15) & ~(size_t)15; }

static V vow_obj(uint8_t kind, uint8_t flags, uint16_t words, uint64_t mask, size_t payload) {
    VHdr *h = (VHdr *)vow_raw_alloc(vow_round(sizeof(VHdr) + payload));
    h->rc = 1;
    h->kind = kind;
    h->flags = flags;
    h->words = words;
    h->mask = mask;
    vow_live++;
    return (V)(uintptr_t)((char *)h + sizeof(VHdr));
}

static V vow_immortal(V x) {
    VH(x)->rc = -1;
    vow_immortals++;
    return x;
}

#ifdef VOW_RC_DEBUG
#define VOW_ALIVE(h) \
    do {             \
        if ((h)->rc == 0) vow_panic("use after free (reference counting bug)"); \
    } while (0)
#else
#define VOW_ALIVE(h) ((void)0)
#endif

static inline V vdup(V x) {
    if (x) {
        VHdr *h = VH(x);
        VOW_ALIVE(h);
        if (h->rc > 0) h->rc++;
    }
    return x;
}

static void vow_destroy(V x);

static inline void vdrop(V x) {
    if (x) {
        VHdr *h = VH(x);
        VOW_ALIVE(h);
        if (h->rc > 0 && --h->rc == 0) vow_destroy(x);
    }
}

// ---- strings ----

typedef struct {
    int64_t len;
    char data[];
} VStr;

#define VS(x) ((VStr *)(uintptr_t)(x))

// ---- lists: a header (length, buffer) sharing a buffer of elements ----

typedef struct {
    int64_t len;
    V buf;
} VList;

typedef struct {
    int64_t cap;
    int64_t used;
    V items[];
} VBuf;

#define VL(x) ((VList *)(uintptr_t)(x))
#define VB(x) ((VBuf *)(uintptr_t)(x))

// ---- maps: persistent AVL trees, updated in place where unshared ----

typedef struct {
    V key, val, left, right;
    int64_t height, size;
} VNode;

#define VN(x) ((VNode *)(uintptr_t)(x))
#define VN_KEY_HEAP 1
#define VN_VAL_HEAP 2
#define VN_STR_KEYS 4

// ---- hash maps: open addressing; the header is followed by the hashes (0 marks an
// empty slot), the keys and the values, each `cap` long ----

typedef struct {
    int64_t count;
    int64_t cap;
} VHash;

#define VHM(x) ((VHash *)(uintptr_t)(x))
static inline uint64_t *vhm_hashes(V h) { return (uint64_t *)((char *)(uintptr_t)h + sizeof(VHash)); }
static inline V *vhm_keyarr(V h) { return (V *)(vhm_hashes(h) + VHM(h)->cap); }
static inline V *vhm_valarr(V h) { return vhm_keyarr(h) + VHM(h)->cap; }

static size_t vow_size(VHdr *h, V x) {
    switch (h->kind) {
    case VK_FIELDS: return vow_round(sizeof(VHdr) + (size_t)(h->words > 0 ? h->words : 1) * sizeof(V));
    case VK_STR: return vow_round(sizeof(VHdr) + sizeof(VStr) + (size_t)VS(x)->len + 1);
    case VK_LIST: return vow_round(sizeof(VHdr) + sizeof(VList));
    case VK_BUF: return vow_round(sizeof(VHdr) + sizeof(VBuf) + (size_t)VB(x)->cap * sizeof(V));
    case VK_HASH: return vow_round(sizeof(VHdr) + sizeof(VHash) + (size_t)VHM(x)->cap * 3 * sizeof(V));
    default: return vow_round(sizeof(VHdr) + sizeof(VNode));
    }
}

// Frees an object and, iteratively, everything only it referenced (no recursion, so
// long chains cannot overflow the C stack).
static V *vow_dstack;
static size_t vow_dcap, vow_dlen;

static void vow_dpush(V c) {
    if (vow_dlen == vow_dcap) {
        vow_dcap = vow_dcap ? vow_dcap * 2 : 256;
        vow_dstack = (V *)realloc(vow_dstack, vow_dcap * sizeof(V));
        if (!vow_dstack) vow_panic("out of memory");
    }
    vow_dstack[vow_dlen++] = c;
}

static inline void vow_release(V c) {
    if (!c) return;
    VHdr *h = VH(c);
    VOW_ALIVE(h);
    if (h->rc > 0 && --h->rc == 0) vow_dpush(c);
}

static void vow_free_shell(V x) {
    VHdr *h = VH(x);
    vow_live--;
    vow_raw_free(h, vow_size(h, x));
}

static void vow_destroy(V x) {
    size_t base = vow_dlen;
    vow_dpush(x);
    while (vow_dlen > base) {
        V y = vow_dstack[--vow_dlen];
        VHdr *h = VH(y);
        switch (h->kind) {
        case VK_FIELDS:
            for (int i = 0; i < h->words; i++)
                if (h->mask & ((uint64_t)1 << i)) vow_release(VP(y)[i]);
            break;
        case VK_LIST: vow_release(VL(y)->buf); break;
        case VK_BUF:
            if (h->flags)
                for (int64_t i = 0; i < VB(y)->used; i++) vow_release(VB(y)->items[i]);
            break;
        case VK_NODE:
            if (h->flags & VN_KEY_HEAP) vow_release(VN(y)->key);
            if (h->flags & VN_VAL_HEAP) vow_release(VN(y)->val);
            vow_release(VN(y)->left);
            vow_release(VN(y)->right);
            break;
        case VK_HASH:
            for (int64_t i = 0; i < VHM(y)->cap; i++) {
                if (!vhm_hashes(y)[i]) continue;
                if (h->flags & VN_KEY_HEAP) vow_release(vhm_keyarr(y)[i]);
                if (h->flags & VN_VAL_HEAP) vow_release(vhm_valarr(y)[i]);
            }
            break;
        default: break;
        }
        vow_free_shell(y);
    }
}

static void vow_report_leaks(void) {
    const char *e = getenv("VOW_LEAKS");
    if (e && *e == '1') fprintf(stderr, "vow: %lld objects live at exit\n", (long long)(vow_live - vow_immortals));
}

// ---- integers ----

static inline V vadd_at(V a, V b, VOW_AT) {
    int64_t r;
    if (__builtin_add_overflow(VI(a), VI(b), &r)) vow_panic_at(vow_f, vow_l, "integer overflow in '+'");
    return (V)r;
}
static inline V vsub_at(V a, V b, VOW_AT) {
    int64_t r;
    if (__builtin_sub_overflow(VI(a), VI(b), &r)) vow_panic_at(vow_f, vow_l, "integer overflow in '-'");
    return (V)r;
}
static inline V vmul_at(V a, V b, VOW_AT) {
    int64_t r;
    if (__builtin_mul_overflow(VI(a), VI(b), &r)) vow_panic_at(vow_f, vow_l, "integer overflow in '*'");
    return (V)r;
}
static inline V vneg_at(V a, VOW_AT) {
    if (VI(a) == INT64_MIN) vow_panic_at(vow_f, vow_l, "integer overflow in unary '-'");
    return (V)(-VI(a));
}
// Euclidean division: the remainder is always non-negative (matches SMT-LIB).
static inline V vdiv_at(V a, V b, VOW_AT) {
    VOW_REQUIRE(VI(b) != 0, "division by zero");
    if (VI(a) == INT64_MIN && VI(b) == -1) vow_panic_at(vow_f, vow_l, "integer overflow in '/'");
    int64_t q = VI(a) / VI(b), r = VI(a) % VI(b);
    if (r < 0) q = VI(b) > 0 ? q - 1 : q + 1;
    return (V)q;
}
static inline V vmod_at(V a, V b, VOW_AT) {
    VOW_REQUIRE(VI(b) != 0, "division by zero");
    if (VI(b) == -1) return 0;
    int64_t r = VI(a) % VI(b);
    if (r < 0) r = VI(b) > 0 ? r + VI(b) : r - VI(b);
    return (V)r;
}

#define vadd(a, b) vadd_at((a), (b), __FILE__, __LINE__)
#define vsub(a, b) vsub_at((a), (b), __FILE__, __LINE__)
#define vmul(a, b) vmul_at((a), (b), __FILE__, __LINE__)
#define vneg(a) vneg_at((a), __FILE__, __LINE__)
#define vdiv(a, b) vdiv_at((a), (b), __FILE__, __LINE__)
#define vmod(a, b) vmod_at((a), (b), __FILE__, __LINE__)

// ---- strings (all functions borrow their arguments) ----

static V vstr_new(const char *p, int64_t n) {
    V s = vow_obj(VK_STR, 0, 0, 0, sizeof(VStr) + (size_t)n + 1);
    VS(s)->len = n;
    if (n) memcpy(VS(s)->data, p, (size_t)n);
    VS(s)->data[n] = 0;
    return s;
}

static inline V vstr_len(V s) { return (V)VS(s)->len; }

static V vstr_cat(V a, V b) {
    int64_t n = VS(a)->len + VS(b)->len;
    V s = vow_obj(VK_STR, 0, 0, 0, sizeof(VStr) + (size_t)n + 1);
    VS(s)->len = n;
    memcpy(VS(s)->data, VS(a)->data, (size_t)VS(a)->len);
    memcpy(VS(s)->data + VS(a)->len, VS(b)->data, (size_t)VS(b)->len);
    VS(s)->data[n] = 0;
    return s;
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

static inline V vstr_byte_at(V s, V i, VOW_AT) {
    VOW_REQUIRE(VI(i) >= 0 && VI(i) < VS(s)->len, "string index out of bounds");
    return (V)(unsigned char)VS(s)->data[VI(i)];
}

#define vstr_byte(s, i) vstr_byte_at((s), (i), __FILE__, __LINE__)

static V vstr_slice_at(V s, V from, V to, VOW_AT) {
    VOW_REQUIRE(0 <= VI(from) && VI(from) <= VI(to) && VI(to) <= VS(s)->len, "string slice out of bounds");
    return vstr_new(VS(s)->data + VI(from), VI(to) - VI(from));
}

#define vstr_slice(s, from, to) vstr_slice_at((s), (from), (to), __FILE__, __LINE__)

static V vchar_at(V code, VOW_AT) {
    VOW_REQUIRE(VI(code) >= 0 && VI(code) < 256, "char code out of range");
    char c = (char)VI(code);
    return vstr_new(&c, 1);
}

#define vchar(code) vchar_at((code), __FILE__, __LINE__)

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

// ---- structs, enums and options ----

static V vow_fields(int64_t words, uint64_t mask) {
    size_t n = (size_t)(words > 0 ? words : 1) * sizeof(V);
    V r = vow_obj(VK_FIELDS, 0, (uint16_t)words, mask, n);
    memset(VP(r), 0, n);
    return r;
}

// Consumes base; returns an object the caller owns exclusively: base itself when it was
// unshared, otherwise a copy. The caller then replaces fields (dropping the old ones).
static V vow_with(V base) {
    VHdr *h = VH(base);
    if (h->rc == 1) return base;
    V r = vow_fields(h->words, h->mask);
    memcpy(VP(r), VP(base), (size_t)h->words * sizeof(V));
    for (int i = 0; i < h->words; i++)
        if (h->mask & ((uint64_t)1 << i)) vdup(VP(r)[i]);
    vdrop(base);
    return r;
}

// Takes field i out of base for an update that `with` will write back: when base is
// unshared the field is moved out (its slot left empty), otherwise duplicated.
static V vow_steal(V base, int64_t i) {
    if (VH(base)->rc == 1) {
        V x = VP(base)[i];
        VP(base)[i] = 0;
        return x;
    }
    return vdup(VP(base)[i]);
}

static V vow_variant0(int64_t tag) {
    V r = vow_fields(1, 0);
    VP(r)[0] = (V)tag;
    return r;
}

static V vow_none_obj;

static V vnone(void) {
    if (!vow_none_obj) vow_none_obj = vow_immortal(vow_fields(1, 0));
    return vow_none_obj;
}

// Consumes x.
static V vsome(V x, int heap) {
    V r = vow_fields(2, heap ? 2 : 0);
    VP(r)[0] = 1;
    VP(r)[1] = x;
    return r;
}

// Borrows o, consumes d.
static V vvalue_or(V o, V d, int heap) {
    if (VP(o)[0] == 1) {
        V r = VP(o)[1];
        if (heap) {
            vdup(r);
            vdrop(d);
        }
        return r;
    }
    return d;
}

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
    return vsome((V)v, 0);
}

// ---- lists ----

static V vbuf_new(int64_t cap, int heap) {
    if (cap < 4) cap = 4;
    V b = vow_obj(VK_BUF, (uint8_t)heap, 0, 0, sizeof(VBuf) + (size_t)cap * sizeof(V));
    VB(b)->cap = cap;
    VB(b)->used = 0;
    return b;
}

static V vlist_make(int64_t len, V buf, int heap) {
    V l = vow_obj(VK_LIST, (uint8_t)heap, 0, 0, sizeof(VList));
    VL(l)->len = len;
    VL(l)->buf = buf;
    return l;
}

static V vlist_empty(int heap) { return vlist_make(0, 0, heap); }

static inline V vlist_len(V l) { return (V)VL(l)->len; }

// Borrowed.
static inline V vlist_get_at(V l, V i, VOW_AT) {
    VOW_REQUIRE(VI(i) >= 0 && VI(i) < VL(l)->len, "list index out of bounds");
    return VB(VL(l)->buf)->items[VI(i)];
}

#define vlist_get(l, i) vlist_get_at((l), (i), __FILE__, __LINE__)

// Consumes l and x. Appends in place when the buffer has room just past this list's
// end (no other list can see that slot); reuses the header when it is unshared.
static V vlist_push(V l, V x) {
    VList *L = VL(l);
    int heap = VH(l)->flags;
    V b = L->buf;
    int64_t n = L->len;
    if (b && VB(b)->used == n && n < VB(b)->cap) {
        VB(b)->items[n] = x;
        VB(b)->used = n + 1;
        if (VH(l)->rc == 1) {
            L->len = n + 1;
            return l;
        }
        V r = vlist_make(n + 1, vdup(b), heap);
        vdrop(l);
        return r;
    }
    V nb = vbuf_new(n * 2, heap);
    if (n) {
        memcpy(VB(nb)->items, VB(b)->items, (size_t)n * sizeof(V));
        if (VH(l)->rc == 1 && VH(b)->rc == 1) {
            // Sole owner of both: move the elements; release any beyond this list's end.
            if (heap)
                for (int64_t i = n; i < VB(b)->used; i++) vdrop(VB(b)->items[i]);
            VB(b)->used = 0;
        } else if (heap) {
            for (int64_t i = 0; i < n; i++) vdup(VB(b)->items[i]);
        }
    }
    VB(nb)->items[n] = x;
    VB(nb)->used = n + 1;
    if (VH(l)->rc == 1) {
        vdrop(L->buf);
        L->buf = nb;
        L->len = n + 1;
        return l;
    }
    V r = vlist_make(n + 1, nb, heap);
    vdrop(l);
    return r;
}

// Consumes l and x: element i replaced, in place when l and its buffer are unshared.
static V vlist_set_at_at(V l, V i, V x, VOW_AT) {
    VOW_REQUIRE(VI(i) >= 0 && VI(i) < VL(l)->len, "set_at index out of bounds");
    VList *L = VL(l);
    int heap = VH(l)->flags;
    V b = L->buf;
    if (VH(l)->rc == 1 && VH(b)->rc == 1) {
        V old = VB(b)->items[VI(i)];
        VB(b)->items[VI(i)] = x;
        if (heap) vdrop(old);
        return l;
    }
    int64_t n = L->len;
    V nb = vbuf_new(n, heap);
    memcpy(VB(nb)->items, VB(b)->items, (size_t)n * sizeof(V));
    if (heap)
        for (int64_t j = 0; j < n; j++)
            if (j != VI(i)) vdup(VB(nb)->items[j]);
    VB(nb)->items[VI(i)] = x;
    VB(nb)->used = n;
    V r = vlist_make(n, nb, heap);
    vdrop(l);
    return r;
}

#define vlist_set_at(l, i, x) vlist_set_at_at((l), (i), (x), __FILE__, __LINE__)

// Borrows l.
static V vlist_slice_at(V l, V from, V to, VOW_AT) {
    VOW_REQUIRE(0 <= VI(from) && VI(from) <= VI(to) && VI(to) <= VL(l)->len, "list slice out of bounds");
    int heap = VH(l)->flags;
    int64_t n = VI(to) - VI(from);
    V nb = vbuf_new(n, heap);
    if (n) {
        memcpy(VB(nb)->items, VB(VL(l)->buf)->items + VI(from), (size_t)n * sizeof(V));
        if (heap)
            for (int64_t j = 0; j < n; j++) vdup(VB(nb)->items[j]);
    }
    VB(nb)->used = n;
    return vlist_make(n, nb, heap);
}

#define vlist_slice(l, from, to) vlist_slice_at((l), (from), (to), __FILE__, __LINE__)

// Consumes a, borrows b.
static V vlist_cat(V a, V b) {
    int heap = VH(a)->flags;
    for (int64_t i = 0; i < VL(b)->len; i++) {
        V x = VB(VL(b)->buf)->items[i];
        a = vlist_push(a, heap ? vdup(x) : x);
    }
    return a;
}

// Borrows both.
static V vjoin(V parts, V sep) {
    int64_t n = 0, k = VL(parts)->len;
    for (int64_t i = 0; i < k; i++) n += VS(VB(VL(parts)->buf)->items[i])->len;
    if (k > 1) n += (k - 1) * VS(sep)->len;
    V s = vow_obj(VK_STR, 0, 0, 0, sizeof(VStr) + (size_t)n + 1);
    VS(s)->len = n;
    char *p = VS(s)->data;
    for (int64_t i = 0; i < k; i++) {
        if (i) {
            memcpy(p, VS(sep)->data, (size_t)VS(sep)->len);
            p += VS(sep)->len;
        }
        V x = VB(VL(parts)->buf)->items[i];
        memcpy(p, VS(x)->data, (size_t)VS(x)->len);
        p += VS(x)->len;
    }
    *p = 0;
    return s;
}

// ---- maps ----

static inline int64_t vn_h(V n) { return n ? VN(n)->height : 0; }
static inline int64_t vn_s(V n) { return n ? VN(n)->size : 0; }

static int vkey_cmp(V a, V b, int flags) {
    if (flags & VN_STR_KEYS) return vstr_cmp(a, b);
    return VI(a) < VI(b) ? -1 : (VI(a) > VI(b) ? 1 : 0);
}

// Consumes all four parts.
static V vn_make(V k, V v, V l, V r, int flags) {
    V n = vow_obj(VK_NODE, (uint8_t)flags, 0, 0, sizeof(VNode));
    VN(n)->key = k;
    VN(n)->val = v;
    VN(n)->left = l;
    VN(n)->right = r;
    int64_t hl = vn_h(l), hr = vn_h(r);
    VN(n)->height = (hl > hr ? hl : hr) + 1;
    VN(n)->size = vn_s(l) + vn_s(r) + 1;
    return n;
}

static void vn_fix(V n) {
    VNode *x = VN(n);
    int64_t hl = vn_h(x->left), hr = vn_h(x->right);
    x->height = (hl > hr ? hl : hr) + 1;
    x->size = vn_s(x->left) + vn_s(x->right) + 1;
}

// Consumes n; returns a node with the same contents that the caller owns exclusively:
// n itself when unshared, otherwise a copy (so updates copy only the path they touch).
static V vn_unshare(V n) {
    if (VH(n)->rc == 1) return n;
    int f = VH(n)->flags;
    VNode *x = VN(n);
    V m = vn_make((f & VN_KEY_HEAP) ? vdup(x->key) : x->key, (f & VN_VAL_HEAP) ? vdup(x->val) : x->val,
                  vdup(x->left), vdup(x->right), f);
    vdrop(n);
    return m;
}

// Rotations relink nodes in place; n must be exclusively owned.
static V vn_rot_right(V n) {
    V l = vn_unshare(VN(n)->left);
    VN(n)->left = VN(l)->right;
    vn_fix(n);
    VN(l)->right = n;
    vn_fix(l);
    return l;
}

static V vn_rot_left(V n) {
    V r = vn_unshare(VN(n)->right);
    VN(n)->right = VN(r)->left;
    vn_fix(n);
    VN(r)->left = n;
    vn_fix(r);
    return r;
}

static V vn_rebalance(V n) {
    VNode *x = VN(n);
    int64_t hl = vn_h(x->left), hr = vn_h(x->right);
    if (hl > hr + 1) {
        if (vn_h(VN(x->left)->left) < vn_h(VN(x->left)->right)) x->left = vn_rot_left(vn_unshare(x->left));
        return vn_rot_right(n);
    }
    if (hr > hl + 1) {
        if (vn_h(VN(x->right)->right) < vn_h(VN(x->right)->left)) x->right = vn_rot_right(vn_unshare(x->right));
        return vn_rot_left(n);
    }
    vn_fix(n);
    return n;
}

// Consumes n, k and v. Unshared nodes on the path are updated in place; shared ones
// are copied first.
static V vn_insert(V n, V k, V v, int flags) {
    if (!n) return vn_make(k, v, 0, 0, flags);
    int c = vkey_cmp(k, VN(n)->key, flags);
    n = vn_unshare(n);
    VNode *x = VN(n);
    if (c == 0) {
        if (flags & VN_KEY_HEAP) vdrop(x->key);
        if (flags & VN_VAL_HEAP) vdrop(x->val);
        x->key = k;
        x->val = v;
        return n;
    }
    if (c < 0)
        x->left = vn_insert(x->left, k, v, flags);
    else
        x->right = vn_insert(x->right, k, v, flags);
    return vn_rebalance(n);
}

static V vmap_empty(void) { return 0; }
static inline V vmap_len(V m) { return (V)vn_s(m); }

static V vmap_insert(V m, V k, V v, int flags) { return vn_insert(m, k, v, flags); }

static V vn_find(V m, V k, int flags) {
    V n = m;
    while (n) {
        int c = vkey_cmp(k, VN(n)->key, flags);
        if (c == 0) return n;
        n = c < 0 ? VN(n)->left : VN(n)->right;
    }
    return 0;
}

// Borrows m and k; the result owns a reference to the value.
static V vmap_lookup(V m, V k, int flags) {
    V n = vn_find(m, k, flags);
    if (!n) return vnone();
    V v = VN(n)->val;
    if (flags & VN_VAL_HEAP) vdup(v);
    return vsome(v, flags & VN_VAL_HEAP);
}

static V vmap_has(V m, V k, int flags) { return (V)(vn_find(m, k, flags) != 0); }

static V vn_keys(V n, V acc, int flags) {
    if (!n) return acc;
    acc = vn_keys(VN(n)->left, acc, flags);
    V k = VN(n)->key;
    acc = vlist_push(acc, (flags & VN_KEY_HEAP) ? vdup(k) : k);
    return vn_keys(VN(n)->right, acc, flags);
}

static V vmap_keys(V m, int flags) { return vn_keys(m, vlist_empty(flags & VN_KEY_HEAP), flags); }

// ---- hash maps ----

static int vow_name_cmp(const void *a, const void *b);

static uint64_t vhm_hash(V k, int flags) {
    uint64_t h;
    if (flags & VN_STR_KEYS) {
        h = 14695981039346656037ull;
        for (int64_t i = 0; i < VS(k)->len; i++) {
            h ^= (unsigned char)VS(k)->data[i];
            h *= 1099511628211ull;
        }
    } else {
        h = k;
        h ^= h >> 30;
        h *= 0xbf58476d1ce4e5b9ull;
        h ^= h >> 27;
        h *= 0x94d049bb133111ebull;
        h ^= h >> 31;
    }
    return h ? h : 1;
}

static inline int vhm_eq(V a, V b, int flags) { return (flags & VN_STR_KEYS) ? (int)vstr_eq(a, b) : a == b; }

static V vhm_new(int64_t cap, int flags) {
    V h = vow_obj(VK_HASH, (uint8_t)flags, 0, 0, sizeof(VHash) + (size_t)cap * 3 * sizeof(V));
    VHM(h)->count = 0;
    VHM(h)->cap = cap;
    memset(vhm_hashes(h), 0, (size_t)cap * sizeof(uint64_t));
    return h;
}

// Consumes k and v; h must be exclusively owned and have a free slot.
static void vhm_place(V h, uint64_t hh, V k, V v, int flags) {
    int64_t cap = VHM(h)->cap, i = (int64_t)(hh & (uint64_t)(cap - 1));
    uint64_t *hs = vhm_hashes(h);
    V *ks = vhm_keyarr(h), *vs = vhm_valarr(h);
    while (hs[i]) {
        if (hs[i] == hh && vhm_eq(ks[i], k, flags)) {
            if (flags & VN_KEY_HEAP) vdrop(k);
            if (flags & VN_VAL_HEAP) vdrop(vs[i]);
            vs[i] = v;
            return;
        }
        i = (i + 1) & (cap - 1);
    }
    hs[i] = hh;
    ks[i] = k;
    vs[i] = v;
    VHM(h)->count++;
}

// Consumes h (exclusively owned): moves its entries into a table of capacity cap.
static V vhm_grow(V h, int64_t cap, int flags) {
    V n = vhm_new(cap, flags);
    for (int64_t i = 0; i < VHM(h)->cap; i++)
        if (vhm_hashes(h)[i]) vhm_place(n, vhm_hashes(h)[i], vhm_keyarr(h)[i], vhm_valarr(h)[i], flags);
    vow_free_shell(h);
    return n;
}

// Consumes h, k and v. An unshared table is updated in place; a shared one is copied
// first (so value semantics hold).
static V vhm_insert(V h, V k, V v, int flags) {
    uint64_t hh = vhm_hash(k, flags);
    if (!h) {
        h = vhm_new(8, flags);
    } else if (VH(h)->rc != 1) {
        int64_t cap = VHM(h)->cap;
        V c = vhm_new(cap, flags);
        memcpy(vhm_hashes(c), vhm_hashes(h), (size_t)cap * 3 * sizeof(V));
        VHM(c)->count = VHM(h)->count;
        for (int64_t i = 0; i < cap; i++) {
            if (!vhm_hashes(c)[i]) continue;
            if (flags & VN_KEY_HEAP) vdup(vhm_keyarr(c)[i]);
            if (flags & VN_VAL_HEAP) vdup(vhm_valarr(c)[i]);
        }
        vdrop(h);
        h = c;
    }
    if ((VHM(h)->count + 1) * 4 > VHM(h)->cap * 3) h = vhm_grow(h, VHM(h)->cap * 2, flags);
    vhm_place(h, hh, k, v, flags);
    return h;
}

static int64_t vhm_find(V h, V k, int flags) {
    if (!h) return -1;
    uint64_t hh = vhm_hash(k, flags);
    int64_t cap = VHM(h)->cap, i = (int64_t)(hh & (uint64_t)(cap - 1));
    while (vhm_hashes(h)[i]) {
        if (vhm_hashes(h)[i] == hh && vhm_eq(vhm_keyarr(h)[i], k, flags)) return i;
        i = (i + 1) & (cap - 1);
    }
    return -1;
}

// Borrows h and k; the result owns a reference to the value.
static V vhm_lookup(V h, V k, int flags) {
    int64_t i = vhm_find(h, k, flags);
    if (i < 0) return vnone();
    V v = vhm_valarr(h)[i];
    if (flags & VN_VAL_HEAP) vdup(v);
    return vsome(v, flags & VN_VAL_HEAP);
}

static V vhm_has(V h, V k, int flags) { return (V)(vhm_find(h, k, flags) >= 0); }
static inline V vhm_len(V h) { return h ? (V)VHM(h)->count : 0; }

static int vow_int_cmp(const void *a, const void *b) {
    int64_t x = VI(*(const V *)a), y = VI(*(const V *)b);
    return x < y ? -1 : (x > y ? 1 : 0);
}

// The keys in ascending order, so programs stay deterministic.
static V vhm_keys(V h, int flags) {
    V l = vlist_empty(flags & VN_KEY_HEAP);
    if (!h) return l;
    int64_t n = 0;
    V *ks = (V *)malloc(sizeof(V) * (size_t)(VHM(h)->count + 1));
    for (int64_t i = 0; i < VHM(h)->cap; i++)
        if (vhm_hashes(h)[i]) ks[n++] = vhm_keyarr(h)[i];
    qsort(ks, (size_t)n, sizeof(V), (flags & VN_STR_KEYS) ? vow_name_cmp : vow_int_cmp);
    for (int64_t i = 0; i < n; i++) l = vlist_push(l, (flags & VN_KEY_HEAP) ? vdup(ks[i]) : ks[i]);
    free(ks);
    return l;
}

// ---- effects (all borrow their arguments) ----

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
    V s = vow_obj(VK_STR, 0, 0, 0, sizeof(VStr) + (size_t)n + 1);
    VS(s)->len = (int64_t)fread(VS(s)->data, 1, (size_t)n, f);
    VS(s)->data[VS(s)->len] = 0;
    fclose(f);
    return vsome(s, 1);
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

static int vow_name_cmp(const void *a, const void *b) { return vstr_cmp(*(const V *)a, *(const V *)b); }

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
    V l = vlist_empty(1);
    for (int64_t i = 0; i < n; i++) l = vlist_push(l, names[i]);
    free(names);
    return vsome(l, 1);
}

static V vargs(void) {
    V l = vlist_empty(1);
    for (int i = 1; i < vow_argc; i++) l = vlist_push(l, vstr_new(vow_argv[i], (int64_t)strlen(vow_argv[i])));
    return l;
}

static V vgetenv(V name) {
    const char *v = getenv(VS(name)->data);
    if (!v) return vnone();
    return vsome(vstr_new(v, (int64_t)strlen(v)), 1);
}

static __attribute__((noreturn)) void vexit(V code) {
    fflush(stdout);
    exit((int)VI(code));
}

// run(argv, input): stdin comes from a temporary file so that large inputs and outputs
// cannot deadlock on full pipes; stdout is captured, stderr is inherited.
static V vrun(V argv, V input) {
    int64_t n = VL(argv)->len;
    V out = vow_fields(2, 2);
    VP(out)[0] = (V)(int64_t)-1;
    char tmpl[] = "/tmp/vow-run-XXXXXX";
    int in_fd = n == 0 ? -1 : mkstemp(tmpl);
    if (in_fd < 0) {
        VP(out)[1] = vstr_new("", 0);
        return out;
    }
    unlink(tmpl);
    int pfd[2];
    if (write(in_fd, VS(input)->data, (size_t)VS(input)->len) != (ssize_t)VS(input)->len || pipe(pfd) != 0) {
        close(in_fd);
        VP(out)[1] = vstr_new("", 0);
        return out;
    }
    lseek(in_fd, 0, SEEK_SET);
    fflush(stdout);
    pid_t pid = fork();
    if (pid == 0) {
        dup2(in_fd, 0);
        dup2(pfd[1], 1);
        close(pfd[0]);
        close(pfd[1]);
        close(in_fd);
        char **av = (char **)malloc(sizeof(char *) * (size_t)(n + 1));
        for (int64_t i = 0; i < n; i++) av[i] = VS(VB(VL(argv)->buf)->items[i])->data;
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
