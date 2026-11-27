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
#include <ctype.h>
#include <dirent.h>
#include <errno.h>
#include <math.h>
#include <fcntl.h>
#include <float.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>
#include <sys/resource.h>
#include <sys/select.h>
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

// Memory for the runtime's own buffers: running out is a panic, as it is for objects.
static void *vow_xmalloc(size_t n) {
    void *p = malloc(n > 0 ? n : 1);
    if (!p) vow_panic("out of memory");
    return p;
}

static void *vow_xrealloc(void *old, size_t n) {
    void *p = realloc(old, n > 0 ? n : 1);
    if (!p) vow_panic("out of memory");
    return p;
}

// ---- objects ----

enum { VK_FIELDS, VK_STR, VK_LIST, VK_BUF, VK_NODE, VK_HASH };

typedef struct {
    int32_t rc;     // reference count; -1 for immortal objects (literals, None)
    uint8_t kind;   // VK_*
    uint8_t flags;  // lists and buffers: elements are heap values; nodes: see below;
                    // VK_FIELDS: the object is wide (more than 63 words)
    uint16_t words; // VK_FIELDS: number of payload words
    uint64_t mask;  // VK_FIELDS: bit i set when payload word i is a heap value; for a
                    // wide object, a pointer to a static array of such masks instead.
                    // VK_STR: the bytes there is room for, once the string has been
                    // appended to (vstr_append_n); 0 while that is its length
} VHdr;

#define VH(x) ((VHdr *)((char *)(uintptr_t)(x) - sizeof(VHdr)))

// Whether payload word i of a VK_FIELDS object holds a heap value: bit i % 64 of
// vow_bitmap(h)[i / 64].
static inline const uint64_t *vow_bitmap(const VHdr *h) {
    return h->flags ? (const uint64_t *)(uintptr_t)h->mask : &h->mask;
}
#define VOW_HEAP_WORD(m, i) (((m)[(i) >> 6] >> ((i) & 63)) & 1)

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
    case VK_STR: return vow_round(sizeof(VHdr) + sizeof(VStr) + (size_t)(h->mask ? (int64_t)h->mask : VS(x)->len) + 1);
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
        case VK_FIELDS: {
            const uint64_t *m = vow_bitmap(h);
            for (int i = 0; i < h->words; i++)
                if (VOW_HEAP_WORD(m, i)) vow_release(VP(y)[i]);
            break;
        }
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

// ---- deep comparison ----

// The equality and rank functions generated for a type (veq_T, vrank_T) do not recurse,
// so a long chain cannot overflow the C stack: a step function looks at one object and
// pushes the parts still to visit.
typedef int (*VStep)(V a, V b);
typedef struct {
    VStep step;
    V a, b;
} VFrame;
static VFrame *vow_work;
static size_t vow_wcap, vow_wlen;

static void vow_work_push(VStep step, V a, V b) {
    if (vow_wlen == vow_wcap) {
        vow_wcap = vow_wcap ? vow_wcap * 2 : 256;
        vow_work = (VFrame *)realloc(vow_work, vow_wcap * sizeof(VFrame));
        if (!vow_work) vow_panic("out of memory");
    }
    vow_work[vow_wlen].step = step;
    vow_work[vow_wlen].a = a;
    vow_work[vow_wlen].b = b;
    vow_wlen++;
}

// Runs step on (a, b), then on everything pushed, until a step answers 0.
static V vow_work_run(VStep step, V a, V b) {
    size_t base = vow_wlen;
    int ok = step(a, b);
    while (ok && vow_wlen > base) {
        VFrame f = vow_work[--vow_wlen];
        ok = f.step(f.a, f.b);
    }
    vow_wlen = base;
    return (V)ok;
}

// A rank step is given a value a and, in d, twice the number of enums and lists
// entered above it; it records the greatest number reached, which is the rank of the
// value the run began with. A struct has the rank of its largest field, so entering
// one adds nothing; d is odd once the walk has entered a struct with several fields
// that have ranks, any of which may be the largest.
static int64_t vow_rank_max;

// A bounded search (vow_rank_cmp) for the value vow_rank_seek below the value a run
// began with: whether it was found (1: with a smaller rank; 2: with the same rank), and
// the steps left. A full run has vow_rank_left negative.
static V vow_rank_seek;
static int vow_rank_found;
static int64_t vow_rank_left = -1;
#define VOW_RANK_GOING (vow_rank_left < 0 || (!vow_rank_found && vow_rank_left > 0))

static inline void vow_rank_meet(V a, V d) {
    if (a != vow_rank_seek || vow_rank_found) return;
    // Below an enum or a list: smaller. Reached only through structs that each have
    // one field with a rank: the same.
    if (VI(d) >= 2) vow_rank_found = 1;
    else if (VI(d) == 0) vow_rank_found = 2;
}

// Every rank step begins with this: 1 when the run should go no further.
static inline int vow_rank_skip(V a, V d) {
    if (vow_rank_left < 0) return 0;
    vow_rank_meet(a, d);
    if (vow_rank_found || vow_rank_left == 0) return 1;
    vow_rank_left--;
    return 0;
}

// Looks at a part as it is pushed, so that all the parts of a value are seen before
// the walk goes deep into the last of them.
static inline void vow_rank_push(VStep step, V a, V d) {
    if (vow_rank_left >= 0) vow_rank_meet(a, d);
    vow_work_push(step, a, d);
}

static int64_t vow_rank_run(VStep step, V a) {
    vow_rank_max = 0;
    vow_work_run(step, a, 0);
    return vow_rank_max;
}

// How the rank of a compares with the rank of e: negative, zero or positive. A
// recursive function is nearly always called on a part of its own argument, and then
// the answer needs no ranks: a value has the rank it has (a is e), and a part of it
// has a smaller rank, or for the one ranked field of a struct the same. Only when a
// short walk of e does not meet a are the two ranks computed; *re keeps e's (negative
// until then). Values with a rank are pointers, so the same bits are the same value.
static int vow_rank_cmp(VStep sa, V a, VStep se, V e, int64_t *re) {
    if (a == e) return 0;
    vow_rank_seek = a;
    vow_rank_found = 0;
    vow_rank_left = 256;
    vow_work_run(se, e, 0);
    int found = vow_rank_found;
    vow_rank_found = 0;
    vow_rank_left = -1;
    if (found) return found == 1 ? -1 : 0;
    if (*re < 0) *re = vow_rank_run(se, e);
    int64_t ra = vow_rank_run(sa, a);
    return ra < *re ? -1 : ra > *re;
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

// ---- floats: a Float is a double stored in the bits of a V ----

// Every Float operation is rounded on its own, as written: the C compiler may not fuse a
// multiply and an add (which would change results).
#pragma STDC FP_CONTRACT OFF

static inline double VD(V x) {
    double d;
    memcpy(&d, &x, sizeof d);
    return d;
}
static inline V DV(double d) {
    V x;
    memcpy(&x, &d, sizeof x);
    return x;
}
// Wide integers, for the arithmetic a checked build does while it evaluates a
// specification. A specification is proved over the mathematical integers, so what it
// writes with +, -, *, / and % is evaluated in 128 bits there: `(result * 2) <= (lo +
// hi)` holds for every Int, and its sum must not trap as the program's own would. Each
// operation gives 1 when even 128 bits cannot hold its result.
#ifdef __SIZEOF_INT128__
typedef __int128 vow_wide;
typedef unsigned __int128 vow_uwide;
#define VOW_WIDE_BITS 128
#else
typedef int64_t vow_wide;
typedef uint64_t vow_uwide;
#define VOW_WIDE_BITS 64
#endif
#define vow_wfits(w) ((w) >= (vow_wide)INT64_MIN && (w) <= (vow_wide)INT64_MAX)
static inline int vow_wadd(vow_wide a, vow_wide b, vow_wide *r) { return __builtin_add_overflow(a, b, r); }
static inline int vow_wsub(vow_wide a, vow_wide b, vow_wide *r) { return __builtin_sub_overflow(a, b, r); }
static inline int vow_wmul(vow_wide a, vow_wide b, vow_wide *r) {
#ifdef __SIZEOF_INT128__
    // Two 64-bit values, as nearly always: the product fits. Otherwise compare
    // magnitudes by division (a compiler's built-in check may call a library function
    // that not every C runtime has).
    if (vow_wfits(a) && vow_wfits(b)) { *r = a * b; return 0; }
    if (a == 0 || b == 0) { *r = 0; return 0; }
    vow_uwide ua = a < 0 ? (vow_uwide)0 - (vow_uwide)a : (vow_uwide)a;
    vow_uwide ub = b < 0 ? (vow_uwide)0 - (vow_uwide)b : (vow_uwide)b;
    int neg = (a < 0) != (b < 0);
    vow_uwide top = (vow_uwide)1 << (VOW_WIDE_BITS - 1);
    if (ua > (neg ? top : top - 1) / ub) return 1;
    vow_uwide p = ua * ub;
    *r = neg ? (vow_wide)((vow_uwide)0 - p) : (vow_wide)p;
    return 0;
#else
    return __builtin_mul_overflow(a, b, r);
#endif
}
// Euclidean, as vdiv and vmod are; b is not zero.
static inline int vow_wdiv(vow_wide a, vow_wide b, vow_wide *r) {
    if (b == -1) return vow_wsub(0, a, r);
    vow_wide q = a / b, m = a % b;
    if (m < 0) q = b > 0 ? q - 1 : q + 1;
    *r = q;
    return 0;
}
static inline int vow_wmod(vow_wide a, vow_wide b, vow_wide *r) {
    if (b == -1) { *r = 0; return 0; }
    vow_wide m = a % b;
    if (m < 0) m = b > 0 ? m + b : m - b;
    *r = m;
    return 0;
}

// IEEE arithmetic: never panics (division by zero gives an infinity or NaN).
static inline V vfadd(V a, V b) { return DV(VD(a) + VD(b)); }
static inline V vfsub(V a, V b) { return DV(VD(a) - VD(b)); }
static inline V vfmul(V a, V b) { return DV(VD(a) * VD(b)); }
static inline V vfdiv(V a, V b) { return DV(VD(a) / VD(b)); }
static inline V vfneg(V a) { return DV(-VD(a)); }
static inline V vint_to_float(V n) { return DV((double)VI(n)); }
static inline V vfloor(V x) { return DV(floor(VD(x))); }
static inline V vceil(V x) { return DV(ceil(VD(x))); }
static inline V vround(V x) { return DV(round(VD(x))); }
static inline V vsqrt(V x) { return DV(sqrt(VD(x))); }
static inline V vabs_float(V x) { return DV(fabs(VD(x))); }
static inline V vis_nan(V x) { return (V)(isnan(VD(x)) != 0); }

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

// a followed by the k strings at parts. Consumes a, borrows the parts (any of which may
// be a itself). When nothing else refers to a it is extended where it is, if its block
// has room: `set s = s ++ x` in a loop is then linear, not quadratic. A string that has
// outgrown its block once is given twice the room it needs the next time; before that,
// and when a is shared, the result is exactly as long as it must be.
static V vstr_append_n(V a, int64_t k, const V *parts) {
    int64_t la = VS(a)->len, need = la;
    for (int64_t i = 0; i < k; i++) need += VS(parts[i])->len;
    VHdr *h = VH(a);
    V s = a;
    if (h->rc == 1) {
        size_t fixed = sizeof(VHdr) + sizeof(VStr) + 1;
        int64_t room = (int64_t)(vow_round(fixed + (size_t)(h->mask ? (int64_t)h->mask : la)) - fixed);
        if (need <= room) {
            h->mask = (uint64_t)room;
        } else {
            int64_t cap = h->mask ? need * 2 : need;
            cap = (int64_t)(vow_round(fixed + (size_t)cap) - fixed);
            s = vow_obj(VK_STR, 0, 0, (uint64_t)cap, sizeof(VStr) + (size_t)cap + 1);
            memcpy(VS(s)->data, VS(a)->data, (size_t)la);
        }
    } else {
        s = vow_obj(VK_STR, 0, 0, 0, sizeof(VStr) + (size_t)need + 1);
        memcpy(VS(s)->data, VS(a)->data, (size_t)la);
    }
    // a's length is set last: a part that is a itself is still read at its old length.
    char *at = VS(s)->data + la;
    for (int64_t i = 0; i < k; i++) {
        size_t n = (size_t)VS(parts[i])->len;
        memcpy(at, VS(parts[i])->data, n);
        at += n;
    }
    VS(s)->len = need;
    VS(s)->data[need] = 0;
    if (s != a) vdrop(a);
    return s;
}

static V vstr_append(V a, V b) { return vstr_append_n(a, 1, &b); }

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

// The shortest text that reads back as the same double, always with a '.' or an
// exponent (1.0, not 1): 0.1, 300.0, 1e+16, 1.5e-05; "nan", "inf" or "-inf".
static V vfloat_to_str(V x) {
    double d = VD(x);
    char buf[48];
    if (isnan(d)) return vstr_new("nan", 3);
    if (isinf(d)) return d > 0 ? vstr_new("inf", 3) : vstr_new("-inf", 4);
    // The fewest significant digits that read back as d; then, as Python's repr does,
    // plain notation for magnitudes from 1e-4 up to 1e16 and an exponent otherwise.
    // A double holds any 15 digits, so if d rounded to 15 reads back as d, the fewest
    // are those 15 without their trailing zeros: any shorter text that reads back as
    // d is d rounded to 15 digits too (two such decimals are further apart than the
    // doubles around d). If not, 16 are the fewest when they read back, and 17 always
    // do. Subnormal numbers hold fewer digits, so there each count is tried.
    int p = 1;
    if (d == 0 || fabs(d) >= DBL_MIN) {
        snprintf(buf, sizeof buf, "%.14e", d);
        if (strtod(buf, NULL) == d) {
            // d.dddddddddddddde+xx, or -d.ddd...: the digits end before the 'e'.
            const char *e = strchr(buf, 'e');
            p = 15;
            while (p > 1 && e[p - 16] == '0') p--;
        } else {
            snprintf(buf, sizeof buf, "%.15e", d);
            p = strtod(buf, NULL) == d ? 16 : 17;
        }
    } else {
        for (; p < 17; p++) {
            snprintf(buf, sizeof buf, "%.*e", p - 1, d);
            if (strtod(buf, NULL) == d) break;
        }
    }
    snprintf(buf, sizeof buf, "%.*e", p - 1, d);
    int e = atoi(strchr(buf, 'e') + 1);
    if (d != 0 && (e < -4 || e >= 16)) {
        // Python's form: 1e+16, 6.02e+23, 1.5e-05.
        char mant[32];
        int len = (int)(strchr(buf, 'e') - buf);
        memcpy(mant, buf, (size_t)len);
        mant[len] = 0;
        snprintf(buf, sizeof buf, "%se%c%02d", mant, e < 0 ? '-' : '+', e < 0 ? -e : e);
    } else {
        int decimals = p - 1 - e;
        if (decimals < 1) decimals = 1;
        snprintf(buf, sizeof buf, "%.*f", decimals, d);
    }
    return vstr_new(buf, (int64_t)strlen(buf));
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

// ---- structs, enums and options ----

static V vow_fields(int64_t words, uint64_t mask) {
    size_t n = (size_t)(words > 0 ? words : 1) * sizeof(V);
    V r = vow_obj(VK_FIELDS, 0, (uint16_t)words, mask, n);
    memset(VP(r), 0, n);
    return r;
}

// An object of more than 63 words: bitmap has one mask for each 64 of them, and
// outlives the object.
static V vow_fields_wide(int64_t words, const uint64_t *bitmap) {
    size_t n = (size_t)words * sizeof(V);
    V r = vow_obj(VK_FIELDS, 1, (uint16_t)words, (uint64_t)(uintptr_t)bitmap, n);
    memset(VP(r), 0, n);
    return r;
}

// Consumes base; returns an object the caller owns exclusively: base itself when it was
// unshared, otherwise a copy. The caller then replaces fields (dropping the old ones).
static V vow_with(V base) {
    VHdr *h = VH(base);
    if (h->rc == 1) return base;
    V r = vow_fields(h->words, h->mask);
    VH(r)->flags = h->flags;
    memcpy(VP(r), VP(base), (size_t)h->words * sizeof(V));
    const uint64_t *m = vow_bitmap(h);
    for (int i = 0; i < h->words; i++)
        if (VOW_HEAP_WORD(m, i)) vdup(VP(r)[i]);
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

// A variant without fields: one object for each tag, shared by every use and never
// freed, as None is (a variant is only its tag, so variants of different enums with
// the same tag share one too). Beyond the 256th variant of an enum each use allocates.
#define VOW_VARIANT0_SHARED 256
static V vow_variant0_obj[VOW_VARIANT0_SHARED];

static V vow_variant0(int64_t tag) {
    if (tag >= 0 && tag < VOW_VARIANT0_SHARED) {
        if (!vow_variant0_obj[tag]) {
            V r = vow_immortal(vow_fields(1, 0));
            VP(r)[0] = (V)tag;
            vow_variant0_obj[tag] = r;
        }
        return vow_variant0_obj[tag];
    }
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

// x with exactly digits digits after the point (0 to 40), as C's printf("%.*f") writes it.
static V vfloat_to_fixed(V x, V digits) {
    int64_t k = VI(digits);
    if (k < 0) k = 0;
    if (k > 40) k = 40;
    int n = snprintf(NULL, 0, "%.*f", (int)k, VD(x));
    char *buf = (char *)vow_xmalloc((size_t)n + 1);
    snprintf(buf, (size_t)n + 1, "%.*f", (int)k, VD(x));
    V s = vstr_new(buf, n);
    free(buf);
    return s;
}

// The whole string must be a number (as C's strtod reads it).
static V vstr_to_float(V s) {
    const char *p = VS(s)->data;
    if (VS(s)->len == 0 || isspace((unsigned char)p[0])) return vnone();
    char *end;
    double d = strtod(p, &end);
    if (end != p + VS(s)->len) return vnone();
    return vsome(DV(d), 0);
}

// None for NaN and for values outside the Int range; otherwise rounded toward zero.
static V vfloat_to_int(V x) {
    double d = VD(x);
    if (isnan(d) || d < -9223372036854775808.0 || d >= 9223372036854775808.0) return vnone();
    return vsome((V)(int64_t)d, 0);
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
    if (b && VH(l)->rc == 1 && VH(b)->rc == 1 && n < VB(b)->used) {
        // The only list on a buffer that holds elements past its end (it is the
        // beginning of a list that is gone, see vlist_cut_at): they are released, and
        // the new element goes where the first of them was.
        if (heap)
            for (int64_t i = n; i < VB(b)->used; i++) vdrop(VB(b)->items[i]);
        VB(b)->items[n] = x;
        VB(b)->used = n + 1;
        L->len = n + 1;
        return l;
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
// The copy, when l or its buffer is shared.
static __attribute__((noinline)) V vlist_set_at_copy(V l, V i, V x) {
    VList *L = VL(l);
    int heap = VH(l)->flags;
    V b = L->buf;
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

// Inline, since in a loop over a list with one owner it is just a store.
static inline V vlist_set_at_at(V l, V i, V x, VOW_AT) {
    VOW_REQUIRE(VI(i) >= 0 && VI(i) < VL(l)->len, "set_at index out of bounds");
    V b = VL(l)->buf;
    if (VH(l)->rc == 1 && VH(b)->rc == 1) {
        V old = VB(b)->items[VI(i)];
        VB(b)->items[VI(i)] = x;
        if (VH(l)->flags) vdrop(old);
        return l;
    }
    return vlist_set_at_copy(l, i, x);
}

#define vlist_set_at(l, i, x) vlist_set_at_at((l), (i), (x), __FILE__, __LINE__)

// Takes element i out of l for an update that set_at then writes back (`set ps =
// set_at(list: ps, at: i, value: ps[i] with (...))`): when l and its buffer are
// unshared the element is moved out and its slot left empty, so that the update can be
// made where the element is; otherwise it is duplicated. Borrows l.
static inline V vlist_steal_at(V l, V i, VOW_AT) {
    VOW_REQUIRE(VI(i) >= 0 && VI(i) < VL(l)->len, "list index out of bounds");
    V b = VL(l)->buf;
    V x = VB(b)->items[VI(i)];
    if (VH(l)->rc == 1 && VH(b)->rc == 1) {
        VB(b)->items[VI(i)] = 0;
        return x;
    }
    return vdup(x);
}

#define vlist_steal(l, i) vlist_steal_at((l), (i), __FILE__, __LINE__)

// Consumes l. The beginning of a list that nothing else refers to stays where it is
// (`set xs = slice(of: xs, from: 0, to: len(xs) - 1)` takes the last element off in
// constant time): the list is made shorter, and if nothing else shares its buffer the
// elements past the new end are released, so that a push can go there.
static V vlist_cut_at(V l, V from, V to, VOW_AT) {
    VOW_REQUIRE(0 <= VI(from) && VI(from) <= VI(to) && VI(to) <= VL(l)->len, "list slice out of bounds");
    int heap = VH(l)->flags;
    int64_t n = VI(to) - VI(from);
    if (VH(l)->rc == 1 && VI(from) == 0) {
        V b = VL(l)->buf;
        if (b && VH(b)->rc == 1) {
            if (heap)
                for (int64_t j = n; j < VB(b)->used; j++) vdrop(VB(b)->items[j]);
            VB(b)->used = n;
        }
        VL(l)->len = n;
        return l;
    }
    // The beginning of a list that something else refers to (a parameter, in a helper
    // that takes the last element off): a new list on the same buffer, when it is at
    // least half of what the buffer holds, so that a short piece does not keep a long
    // buffer and its elements alive. A push to it then finds elements past its end,
    // and releases them if by then it is the buffer's only list (vlist_push).
    if (VI(from) == 0 && VL(l)->buf && n * 2 >= VB(VL(l)->buf)->used) {
        V r = vlist_make(n, vdup(VL(l)->buf), heap);
        vdrop(l);
        return r;
    }
    V nb = vbuf_new(n, heap);
    if (n) {
        memcpy(VB(nb)->items, VB(VL(l)->buf)->items + VI(from), (size_t)n * sizeof(V));
        if (heap)
            for (int64_t j = 0; j < n; j++) vdup(VB(nb)->items[j]);
    }
    VB(nb)->used = n;
    vdrop(l);
    return vlist_make(n, nb, heap);
}

#define vlist_cut(l, from, to) vlist_cut_at((l), (from), (to), __FILE__, __LINE__)

// Consumes a, borrows b.
static V vlist_cat(V a, V b) {
    int heap = VH(a)->flags;
    // b can be a itself (set xs = xs ++ xs), growing as it is pushed to: its length
    // is read once.
    int64_t n = VL(b)->len;
    for (int64_t i = 0; i < n; i++) {
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

// value_or(opt: lookup(map: m, key: k), default: d) without the Option: borrows m and
// k and consumes d; the result owns a reference to the value.
static V vmap_get_or(V m, V k, V d, int flags) {
    V n = vn_find(m, k, flags);
    if (!n) return d;
    V v = VN(n)->val;
    if (flags & VN_VAL_HEAP) {
        vdup(v);
        vdrop(d);
    }
    return v;
}

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

// As vmap_get_or.
static V vhm_get_or(V h, V k, V d, int flags) {
    int64_t i = vhm_find(h, k, flags);
    if (i < 0) return d;
    V v = vhm_valarr(h)[i];
    if (flags & VN_VAL_HEAP) {
        vdup(v);
        vdrop(d);
    }
    return v;
}
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
    V *ks = (V *)vow_xmalloc(sizeof(V) * (size_t)(VHM(h)->count + 1));
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

// ---- stack overflow ----

// Recursion deeper than the C stack allows is a panic, not a crash: the fault is
// handled on a stack of its own, and an address within the stack's limit (or just past
// it) means the stack ran out. Any other fault is a bug in the runtime or the compiler,
// and ends the program as it would without the handler.
static char vow_fault_stack[65536];
static char *vow_stack_top;
static size_t vow_stack_limit;
// Set while standard output is being written to: flushing it then could deadlock.
static volatile sig_atomic_t vow_stdout_busy;

static void vow_on_fault(int sig, siginfo_t *si, void *context) {
    (void)context;
    char *at = (char *)si->si_addr;
    size_t reach = vow_stack_limit + ((size_t)1 << 20);
    if (at <= vow_stack_top && (size_t)(vow_stack_top - at) <= reach) {
        static const char msg[] = "vow: panic: stack overflow (recursion too deep)\n";
        if (!vow_stdout_busy) fflush(stdout);
        if (write(2, msg, sizeof msg - 1) < 0) _exit(101);
        _exit(101);
    }
    signal(sig, SIG_DFL);
}

// Called first in main, with the address of one of its locals.
static void vow_guard_stack(void *top) {
#if !defined(__SANITIZE_ADDRESS__)
    struct rlimit lim;
    vow_stack_top = (char *)top;
    vow_stack_limit = (size_t)1 << 30;
    if (getrlimit(RLIMIT_STACK, &lim) == 0 && lim.rlim_cur != RLIM_INFINITY) vow_stack_limit = (size_t)lim.rlim_cur;
    stack_t alt;
    alt.ss_sp = vow_fault_stack;
    alt.ss_size = sizeof vow_fault_stack;
    alt.ss_flags = 0;
    if (sigaltstack(&alt, 0) != 0) return;
    struct sigaction act;
    memset(&act, 0, sizeof act);
    act.sa_sigaction = vow_on_fault;
    act.sa_flags = SA_SIGINFO | SA_ONSTACK;
    sigemptyset(&act.sa_mask);
    sigaction(SIGSEGV, &act, 0);
    sigaction(SIGBUS, &act, 0);
#endif
}

// A program whose output is lost has failed: a write to standard output that fails is
// a panic, when it is made or, for buffered output, when the program ends.
static __attribute__((noreturn, cold)) void vow_stdout_failed(void) {
    fprintf(stderr, "vow: panic: cannot write to standard output\n");
    _exit(101);
}

static void vow_flush_stdout(void) {
    if (fflush(stdout) != 0 || ferror(stdout)) vow_stdout_failed();
}

static V vprint(V s) {
    size_t n = (size_t)VS(s)->len;
    vow_stdout_busy = 1;
    size_t w = fwrite(VS(s)->data, 1, n, stdout);
    vow_stdout_busy = 0;
    if (w != n) vow_stdout_failed();
    return 0;
}

static V veprint(V s) {
    fflush(stdout);
    fwrite(VS(s)->data, 1, (size_t)VS(s)->len, stderr);
    return 0;
}

// None for a directory, or when opening or reading fails. Reads to the end of the
// stream: a pipe or a device has no size, and a file's size is only a first guess.
static V vread_file(V path) {
    FILE *f = fopen(VS(path)->data, "rb");
    if (!f) return vnone();
    struct stat st;
    if (fstat(fileno(f), &st) != 0 || S_ISDIR(st.st_mode)) {
        fclose(f);
        return vnone();
    }
    size_t cap = 4096, n = 0;
    if (S_ISREG(st.st_mode) && st.st_size > 0) cap = (size_t)st.st_size + 1;
    char *buf = (char *)vow_xmalloc(cap);
    for (;;) {
        size_t k = fread(buf + n, 1, cap - n, f);
        n += k;
        if (k == 0) break;
        if (n == cap) {
            cap *= 2;
            buf = (char *)vow_xrealloc(buf, cap);
        }
    }
    int failed = ferror(f);
    fclose(f);
    if (failed) {
        free(buf);
        return vnone();
    }
    V s = vow_obj(VK_STR, 0, 0, 0, sizeof(VStr) + n + 1);
    memcpy(VS(s)->data, buf, n);
    VS(s)->data[n] = 0;
    VS(s)->len = (int64_t)n;
    free(buf);
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
    char *p = (char *)vow_xmalloc((size_t)n + 1);
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

// real_path(path): the absolute path of an existing file or directory, with symbolic
// links followed and "." and ".." gone; where file names ignore case, in the case the
// names have on disk.
static V vreal_path(V path) {
    char *r = realpath(VS(path)->data, 0);
    if (!r) return vnone();
    V s = vstr_new(r, (int64_t)strlen(r));
    free(r);
    return vsome(s, 1);
}

static int vow_name_cmp(const void *a, const void *b) { return vstr_cmp(*(const V *)a, *(const V *)b); }

// list_dir(path): the names in a directory, sorted, without "." and "..".
static V vlist_dir(V path) {
    DIR *d = opendir(VS(path)->data);
    if (!d) return vnone();
    int64_t cap = 16, n = 0;
    V *names = (V *)vow_xmalloc(sizeof(V) * (size_t)cap);
    struct dirent *e;
    while ((e = readdir(d)) != 0) {
        if (strcmp(e->d_name, ".") == 0 || strcmp(e->d_name, "..") == 0) continue;
        if (n == cap) names = (V *)vow_xrealloc(names, sizeof(V) * (size_t)(cap *= 2));
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

// Standard input is read through this buffer and not stdio's, so that input_ready can
// tell whether bytes already read from the descriptor are waiting.
static char *vow_in_buf = NULL;
static size_t vow_in_cap = 0, vow_in_pos = 0, vow_in_len = 0;
static int vow_in_ended = 0;

// Reads more of standard input after what is waiting: 0 at its end (or on an error,
// which ends it).
static int vow_in_fill(void) {
    if (vow_in_ended) return 0;
    if (vow_in_pos > 0) {
        memmove(vow_in_buf, vow_in_buf + vow_in_pos, vow_in_len - vow_in_pos);
        vow_in_len -= vow_in_pos;
        vow_in_pos = 0;
    }
    if (vow_in_len == vow_in_cap) {
        vow_in_cap = vow_in_cap ? vow_in_cap * 2 : 65536;
        vow_in_buf = (char *)vow_xrealloc(vow_in_buf, vow_in_cap);
    }
    for (;;) {
        ssize_t k = read(0, vow_in_buf + vow_in_len, vow_in_cap - vow_in_len);
        if (k < 0 && errno == EINTR) continue;
        if (k <= 0) {
            vow_in_ended = 1;
            return 0;
        }
        vow_in_len += (size_t)k;
        return 1;
    }
}

// read_line and read_bytes flush stdout first, so a prompt or a response is out before
// the program waits.
static V vread_line(void) {
    fflush(stdout);
    size_t seen = 0;
    for (;;) {
        size_t have = vow_in_len - vow_in_pos;
        char *from = vow_in_buf + vow_in_pos;
        char *nl = have > seen ? (char *)memchr(from + seen, '\n', have - seen) : NULL;
        if (nl) {
            V s = vstr_new(from, (int64_t)(nl - from));
            vow_in_pos += (size_t)(nl - from) + 1;
            return vsome(s, 1);
        }
        seen = have;
        if (!vow_in_fill()) break;
    }
    // The end of input: what is left is a last line without a newline.
    size_t have = vow_in_len - vow_in_pos;
    if (have == 0) return vnone();
    V s = vstr_new(vow_in_buf + vow_in_pos, (int64_t)have);
    vow_in_pos = vow_in_len;
    return vsome(s, 1);
}

// Up to n bytes (fewer at the end of input).
static V vread_bytes(V n) {
    fflush(stdout);
    size_t want = VI(n) > 0 ? (size_t)VI(n) : 0;
    while (vow_in_len - vow_in_pos < want && vow_in_fill()) {
    }
    size_t got = vow_in_len - vow_in_pos;
    if (got > want) got = want;
    V s = vstr_new(vow_in_buf + vow_in_pos, (int64_t)got);
    vow_in_pos += got;
    return s;
}

// input_ready(ms): whether a read of standard input would have something to give (or
// would find its end) without waiting, once up to ms milliseconds have passed.
static V vinput_ready(V ms) {
    fflush(stdout);
    if (vow_in_pos < vow_in_len || vow_in_ended) return (V)1;
    int64_t wait = VI(ms) > 0 ? VI(ms) : 0;
    for (;;) {
        fd_set fds;
        FD_ZERO(&fds);
        FD_SET(0, &fds);
        struct timeval tv;
        tv.tv_sec = (time_t)(wait / 1000);
        tv.tv_usec = (suseconds_t)((wait % 1000) * 1000);
        int r = select(1, &fds, NULL, NULL, &tv);
        if (r < 0 && errno == EINTR) continue;
        // An error (standard input is closed, say) is found by the read.
        return (V)(r != 0);
    }
}

static V vgetenv(V name) {
    const char *v = getenv(VS(name)->data);
    if (!v) return vnone();
    return vsome(vstr_new(v, (int64_t)strlen(v)), 1);
}

static __attribute__((noreturn)) void vexit(V code) {
    vow_flush_stdout();
    exit((int)VI(code));
}

// run(argv, input): stdin comes from a temporary file (in $TMPDIR, or /tmp) so that
// large inputs and outputs cannot deadlock on full pipes, or from /dev/null when there
// is no input; stdout is captured, stderr is inherited. The code is -1 when the
// command could not be started or did not exit by itself.
static V vrun(V argv, V input) {
    int64_t n = VL(argv)->len;
    V out = vow_fields(2, 2);
    VP(out)[0] = (V)(int64_t)-1;
    int in_fd = -1;
    if (n > 0 && VS(input)->len == 0) {
        in_fd = open("/dev/null", O_RDONLY);
    } else if (n > 0) {
        const char *dir = getenv("TMPDIR");
        if (!dir || !*dir) dir = "/tmp";
        size_t dlen = strlen(dir);
        char *tmpl = (char *)vow_xmalloc(dlen + 32);
        snprintf(tmpl, dlen + 32, "%s/vow-run-XXXXXX", dir);
        in_fd = mkstemp(tmpl);
        if (in_fd >= 0) {
            unlink(tmpl);
            const char *data = VS(input)->data;
            size_t left = (size_t)VS(input)->len;
            while (left > 0) {
                ssize_t w = write(in_fd, data, left);
                if (w < 0 && errno == EINTR) continue;
                if (w <= 0) break;
                data += w;
                left -= (size_t)w;
            }
            if (left > 0 || lseek(in_fd, 0, SEEK_SET) != 0) {
                close(in_fd);
                in_fd = -1;
            }
        }
        free(tmpl);
    }
    int pfd[2];
    if (in_fd < 0 || pipe(pfd) != 0) {
        if (in_fd >= 0) close(in_fd);
        VP(out)[1] = vstr_new("", 0);
        return out;
    }
    // Before the fork: the child must not allocate.
    char **av = (char **)vow_xmalloc(sizeof(char *) * (size_t)(n + 1));
    for (int64_t i = 0; i < n; i++) av[i] = VS(VB(VL(argv)->buf)->items[i])->data;
    av[n] = 0;
    fflush(stdout);
    pid_t pid = fork();
    if (pid == 0) {
        dup2(in_fd, 0);
        dup2(pfd[1], 1);
        close(pfd[0]);
        close(pfd[1]);
        close(in_fd);
        execvp(av[0], av);
        _exit(127);
    }
    free(av);
    close(pfd[1]);
    close(in_fd);
    if (pid < 0) {
        close(pfd[0]);
        VP(out)[1] = vstr_new("", 0);
        return out;
    }
    size_t cap = 4096, len = 0;
    char *buf = (char *)vow_xmalloc(cap);
    for (;;) {
        if (len == cap) buf = (char *)vow_xrealloc(buf, cap *= 2);
        ssize_t k = read(pfd[0], buf + len, cap - len);
        if (k < 0 && errno == EINTR) continue;
        if (k <= 0) break;
        len += (size_t)k;
    }
    close(pfd[0]);
    int status = 0;
    pid_t done;
    do {
        done = waitpid(pid, &status, 0);
    } while (done < 0 && errno == EINTR);
    if (done == pid && WIFEXITED(status)) VP(out)[0] = (V)(int64_t)WEXITSTATUS(status);
    VP(out)[1] = vstr_new(buf, (int64_t)len);
    free(buf);
    return out;
}

// run_attached(argv): the command runs with this program's standard input, output and
// error, and this program waits for it. While it waits it ignores the interrupt and
// quit signals, which a terminal sends to both (as C's system() does), so that it
// outlives the command and can clean up after it. The result is the command's exit
// status: 127 when it was not found, -1 when it could not be started or was ended by a
// signal.
static V vrun_attached(V argv) {
    int64_t n = VL(argv)->len;
    if (n == 0) return (V)(int64_t)-1;
    char **av = (char **)vow_xmalloc(sizeof(char *) * (size_t)(n + 1));
    for (int64_t i = 0; i < n; i++) av[i] = VS(VB(VL(argv)->buf)->items[i])->data;
    av[n] = 0;
    fflush(stdout);
    struct sigaction ignore, old_int, old_quit;
    memset(&ignore, 0, sizeof ignore);
    ignore.sa_handler = SIG_IGN;
    sigemptyset(&ignore.sa_mask);
    sigaction(SIGINT, &ignore, &old_int);
    sigaction(SIGQUIT, &ignore, &old_quit);
    pid_t pid = fork();
    if (pid == 0) {
        sigaction(SIGINT, &old_int, 0);
        sigaction(SIGQUIT, &old_quit, 0);
        execvp(av[0], av);
        _exit(127);
    }
    free(av);
    int64_t code = -1;
    if (pid > 0) {
        int status = 0;
        pid_t done;
        do {
            done = waitpid(pid, &status, 0);
        } while (done < 0 && errno == EINTR);
        if (done == pid && WIFEXITED(status)) code = WEXITSTATUS(status);
    }
    sigaction(SIGINT, &old_int, 0);
    sigaction(SIGQUIT, &old_quit, 0);
    return (V)code;
}

#endif
