#ifndef POINTDEXTER_H
#define POINTDEXTER_H

/*
 * pointdexter.h — C interface to the Pointdexter lock-free tree library.
 *
 * Build the library first:
 *   cargo build --release
 *
 * Then compile your C/C++ program:
 *   gcc  -O2 -o example example.c   -L./target/release -lpointdexter -Wl,-rpath,./target/release
 *   g++  -O2 -o example example.cpp -L./target/release -lpointdexter -Wl,-rpath,./target/release
 *
 * Memory contract
 * ───────────────
 * Every function that returns a *char or *PD_Point allocates heap memory
 * that the caller must free using the corresponding pd_*_free function.
 * Never call free() directly on a pointer returned by this library.
 *
 * Error codes
 * ───────────
 * PD_OK        0   success
 * PD_ERR_NULL  1   required pointer was NULL
 * PD_ERR_SELF  2   tried to attach a point to itself
 * PD_ERR_CYCLE 3   attachment would create a cycle
 * PD_ERR_UTF8  4   input string was not valid UTF-8
 */

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── Opaque handle ───────────────────────────────────────────────────────── */
typedef struct PD_Point PD_Point;

/* ── Error codes ─────────────────────────────────────────────────────────── */
#define PD_OK        0
#define PD_ERR_NULL  1
#define PD_ERR_SELF  2
#define PD_ERR_CYCLE 3
#define PD_ERR_UTF8  4

/* ── Returned collections ────────────────────────────────────────────────── */

/* A heap-allocated array of NUL-terminated strings. Free with pd_string_list_free. */
typedef struct {
    char  **data;
    size_t  len;
} PD_StringList;

/* A heap-allocated array of (key, value) pairs. Free with pd_pair_list_free. */
typedef struct {
    struct { char *key; char *value; } *data;
    size_t len;
} PD_PairList;

/* ── Point lifecycle ─────────────────────────────────────────────────────── */

/* Create or retrieve a Point by name. Returns NULL on error. Free with pd_point_free. */
PD_Point *pd_point(const char *name);

/* Retrieve an existing Point by name, or NULL if not found. Free with pd_point_free. */
PD_Point *pd_get(const char *name);

/* Clone a handle — both original and clone refer to the same logical Point. */
PD_Point *pd_point_clone(PD_Point *pt);

/* Free a Point handle. Safe to call with NULL. */
void pd_point_free(PD_Point *pt);

/* Delete a named Point and its entire subtree from the registry. */
int pd_purge_point(const char *name);

/* ── Entry operations ────────────────────────────────────────────────────── */

/* Insert a key/value entry. Duplicate keys are permitted. O(log E). */
int pd_insert(PD_Point *pt, const char *key, const char *value);

/* Remove all entries with the given key. O(log E + k). */
int pd_purge_key(PD_Point *pt, const char *key);

/* Return all values for a key. Free with pd_string_list_free. O(log E + k). */
PD_StringList pd_get_values(PD_Point *pt, const char *key);

/* Return the first value for a key, or NULL if absent. Free with pd_string_free. O(log E). */
char *pd_get_first(PD_Point *pt, const char *key);

/* Return the name of a Point. Free with pd_string_free. */
char *pd_name(PD_Point *pt);

/* ── Relationships ───────────────────────────────────────────────────────── */

/* Attach child under parent. Re-parents child atomically if it already has a parent. */
int pd_attach(PD_Point *parent, PD_Point *child);

/* Detach this Point from its parent. No-op if already a root. */
int pd_detach(PD_Point *pt);

/* Return the parent, or NULL if this is a root. Free with pd_point_free. */
PD_Point *pd_parent(PD_Point *pt);

/*
 * Return direct children as an array of Point handles.
 * *out_len is set to the number of elements.
 * Free each element with pd_point_free, then the array with pd_point_array_free.
 */
PD_Point **pd_children(PD_Point *pt, size_t *out_len);

/* Free a PD_Point*[] array returned by pd_children (does NOT free the handles). */
void pd_point_array_free(PD_Point **arr, size_t len);

/* ── Search ──────────────────────────────────────────────────────────────── */

/* Global search: (point_name, value) pairs for key across all Points. O(P × log E). */
PD_PairList pd_search_global(const char *key);

/* Scoped search: (point_name, value) pairs within pt's subtree. O(N × log E). */
PD_PairList pd_search(PD_Point *pt, const char *key);

/* ── Traversal ───────────────────────────────────────────────────────────── */

/* Best-effort traversal — tree may change while cb runs. Each handle passed to
   cb is owned by the callback; free it with pd_point_free inside cb. */
void pd_iter_lockfree(void (*cb)(PD_Point *, void *), void *user_data);

/* Synchronized traversal — no structural mutations during cb. */
void pd_iter(void (*cb)(PD_Point *, void *), void *user_data);

/* ── Memory management ───────────────────────────────────────────────────── */

/* Free a char* string returned by any pd_* function. */
void pd_string_free(char *s);

/* Free a PD_StringList and all strings it contains. */
void pd_string_list_free(PD_StringList list);

/* Free a PD_PairList and all strings it contains. */
void pd_pair_list_free(PD_PairList list);

#ifdef __cplusplus
}
#endif

#endif /* POINTDEXTER_H */
