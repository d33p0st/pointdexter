use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};

use crate::core::{AttachError, Point, PointDexter};

// ── Error codes ─────────────────────────────────────────────────────────────

pub const PD_OK: c_int = 0;
pub const PD_ERR_NULL: c_int = 1;
pub const PD_ERR_SELF: c_int = 2;
pub const PD_ERR_CYCLE: c_int = 3;
#[allow(unused)]
pub const PD_ERR_UTF8: c_int = 4;

// ── C-stable structs ─────────────────────────────────────────────────────────

/// A single (key, value) pair returned by search functions.
///
/// Both fields are NUL-terminated UTF-8 strings allocated by Rust.
/// Free the enclosing `PD_PairList` with `pd_pair_list_free` — do not
/// free individual fields.
#[repr(C)]
pub struct PD_StringPair {
    pub key: *mut c_char,
    pub value: *mut c_char,
}

/// A heap-allocated array of NUL-terminated strings.
///
/// Free with `pd_string_list_free`.
#[repr(C)]
pub struct PD_StringList {
    pub data: *mut *mut c_char,
    pub len: usize,
}

/// A heap-allocated array of (key, value) pairs.
///
/// Free with `pd_pair_list_free`.
#[repr(C)]
pub struct PD_PairList {
    pub data: *mut PD_StringPair,
    pub len: usize,
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Convert a `*const c_char` to `&str`.  Returns `None` on null or bad UTF-8.
unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok()
}

/// Leak a Rust `String` as a `*mut c_char` the caller must free with
/// `pd_string_free`.
fn string_to_c(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

/// Wrap a `Point` in a Box and return it as an opaque `*mut Point`.
fn point_to_ptr(p: Point) -> *mut Point {
    Box::into_raw(Box::new(p))
}

/// Borrow a `Point` from an opaque pointer without taking ownership.
///
/// # Safety
/// Caller must ensure the pointer was produced by a `pd_*` function and has
/// not been freed yet.
unsafe fn point_ref<'a>(ptr: *mut Point) -> Option<&'a Point> {
    if ptr.is_null() {
        None
    } else {
        Some(&*ptr)
    }
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

/// Create or retrieve a Point by name.
///
/// Returns an opaque `*mut PD_Point` handle, or NULL on error (null/bad name).
/// The caller is responsible for freeing it with `pd_point_free`.
#[no_mangle]
pub unsafe extern "C" fn pd_point(name: *const c_char) -> *mut Point {
    let Some(n) = cstr_to_str(name) else {
        return std::ptr::null_mut();
    };
    point_to_ptr(PointDexter::new().point(n))
}

/// Look up an existing Point by name.
///
/// Returns NULL if no such Point exists.
/// Free the returned handle with `pd_point_free`.
#[no_mangle]
pub unsafe extern "C" fn pd_get(name: *const c_char) -> *mut Point {
    let Some(n) = cstr_to_str(name) else {
        return std::ptr::null_mut();
    };
    match PointDexter::new().get(n) {
        Some(p) => point_to_ptr(p),
        None => std::ptr::null_mut(),
    }
}

/// Clone a Point handle.
///
/// Both the original and the clone refer to the same underlying Point.
/// Each must be freed independently with `pd_point_free`.
#[no_mangle]
pub unsafe extern "C" fn pd_point_clone(pt: *mut Point) -> *mut Point {
    let Some(p) = point_ref(pt) else {
        return std::ptr::null_mut();
    };
    point_to_ptr(p.clone())
}

/// Free a Point handle obtained from any `pd_*` function.
///
/// Safe to call with NULL (no-op).
#[no_mangle]
pub unsafe extern "C" fn pd_point_free(pt: *mut Point) {
    if !pt.is_null() {
        drop(Box::from_raw(pt));
    }
}

/// Delete a named Point and its entire subtree from the global registry.
#[no_mangle]
pub unsafe extern "C" fn pd_purge_point(name: *const c_char) -> c_int {
    let Some(n) = cstr_to_str(name) else {
        return PD_ERR_NULL;
    };
    PointDexter::new().purge_point(n);
    PD_OK
}

// ── Entry operations ─────────────────────────────────────────────────────────

/// Insert a key/value entry into a Point — O(log E).
///
/// Duplicate keys are permitted.
#[no_mangle]
pub unsafe extern "C" fn pd_insert(
    pt: *mut Point,
    key: *const c_char,
    value: *const c_char,
) -> c_int {
    let Some(p) = point_ref(pt) else {
        return PD_ERR_NULL;
    };
    let Some(k) = cstr_to_str(key) else {
        return PD_ERR_NULL;
    };
    let Some(v) = cstr_to_str(value) else {
        return PD_ERR_NULL;
    };
    p.insert(k, v);
    PD_OK
}

/// Remove all entries with the given key — O(log E + k).
#[no_mangle]
pub unsafe extern "C" fn pd_purge_key(pt: *mut Point, key: *const c_char) -> c_int {
    let Some(p) = point_ref(pt) else {
        return PD_ERR_NULL;
    };
    let Some(k) = cstr_to_str(key) else {
        return PD_ERR_NULL;
    };
    p.purge_key(k);
    PD_OK
}

/// Return all values for a key as a `PD_StringList` — O(log E + k).
///
/// Free with `pd_string_list_free`.
#[no_mangle]
pub unsafe extern "C" fn pd_get_values(pt: *mut Point, key: *const c_char) -> PD_StringList {
    let null_list = PD_StringList {
        data: std::ptr::null_mut(),
        len: 0,
    };
    let Some(p) = point_ref(pt) else {
        return null_list;
    };
    let Some(k) = cstr_to_str(key) else {
        return null_list;
    };
    let vals: Vec<*mut c_char> = p.get(k).into_iter().map(string_to_c).collect();
    let len = vals.len();
    let data = Box::into_raw(vals.into_boxed_slice()) as *mut *mut c_char;
    PD_StringList { data, len }
}

/// Return the first value for a key, or NULL if absent — O(log E).
///
/// Free the returned string with `pd_string_free`.
#[no_mangle]
pub unsafe extern "C" fn pd_get_first(pt: *mut Point, key: *const c_char) -> *mut c_char {
    let Some(p) = point_ref(pt) else {
        return std::ptr::null_mut();
    };
    let Some(k) = cstr_to_str(key) else {
        return std::ptr::null_mut();
    };
    p.get_first(k)
        .map(string_to_c)
        .unwrap_or(std::ptr::null_mut())
}

/// Return the name of a Point.
///
/// Free the returned string with `pd_string_free`.
#[no_mangle]
pub unsafe extern "C" fn pd_name(pt: *mut Point) -> *mut c_char {
    let Some(p) = point_ref(pt) else {
        return std::ptr::null_mut();
    };
    string_to_c(p.name().to_owned())
}

// ── Relationships ────────────────────────────────────────────────────────────

/// Attach `child` under `parent` — O(depth) for cycle check, O(1) otherwise.
///
/// Returns `PD_ERR_SELF` or `PD_ERR_CYCLE` on structural violations.
/// Re-parents `child` atomically if it already has a parent.
#[no_mangle]
pub unsafe extern "C" fn pd_attach(parent: *mut Point, child: *mut Point) -> c_int {
    let Some(par) = point_ref(parent) else {
        return PD_ERR_NULL;
    };
    let Some(chi) = point_ref(child) else {
        return PD_ERR_NULL;
    };
    match par.attach(chi) {
        Ok(()) => PD_OK,
        Err(AttachError::SelfAttach) => PD_ERR_SELF,
        Err(AttachError::WouldCycle) => PD_ERR_CYCLE,
    }
}

/// Detach a Point from its parent — O(1).
#[no_mangle]
pub unsafe extern "C" fn pd_detach(pt: *mut Point) -> c_int {
    let Some(p) = point_ref(pt) else {
        return PD_ERR_NULL;
    };
    p.detach();
    PD_OK
}

/// Return the parent of a Point, or NULL if it is a root.
///
/// Free with `pd_point_free`.
#[no_mangle]
pub unsafe extern "C" fn pd_parent(pt: *mut Point) -> *mut Point {
    let Some(p) = point_ref(pt) else {
        return std::ptr::null_mut();
    };
    p.parent().map(point_to_ptr).unwrap_or(std::ptr::null_mut())
}

/// Return direct children as a list of opaque Point pointers.
///
/// The returned array has `out_len` elements; each element must be freed
/// with `pd_point_free`, then the array itself with `pd_point_array_free`.
#[no_mangle]
pub unsafe extern "C" fn pd_children(pt: *mut Point, out_len: *mut usize) -> *mut *mut Point {
    if out_len.is_null() {
        return std::ptr::null_mut();
    }
    let Some(p) = point_ref(pt) else {
        *out_len = 0;
        return std::ptr::null_mut();
    };
    let children: Vec<*mut Point> = p.children().into_iter().map(point_to_ptr).collect();
    *out_len = children.len();
    Box::into_raw(children.into_boxed_slice()) as *mut *mut Point
}

/// Free a `*mut *mut Point` array returned by `pd_children`.
///
/// Does NOT free the individual Point handles — those must be freed separately
/// with `pd_point_free` before or after calling this.
#[no_mangle]
pub unsafe extern "C" fn pd_point_array_free(arr: *mut *mut Point, len: usize) {
    if arr.is_null() {
        return;
    }
    drop(Box::from_raw(std::slice::from_raw_parts_mut(arr, len)));
}

// ── Search ───────────────────────────────────────────────────────────────────

/// Global search: all (point_name, value) pairs for `key` — O(P × log E).
///
/// Free with `pd_pair_list_free`.
#[no_mangle]
pub unsafe extern "C" fn pd_search_global(key: *const c_char) -> PD_PairList {
    let null_list = PD_PairList {
        data: std::ptr::null_mut(),
        len: 0,
    };
    let Some(k) = cstr_to_str(key) else {
        return null_list;
    };
    pairs_to_ffi(PointDexter::new().search(k))
}

/// Scoped search: all (point_name, value) pairs within `pt`'s subtree.
///
/// Free with `pd_pair_list_free`.
#[no_mangle]
pub unsafe extern "C" fn pd_search(pt: *mut Point, key: *const c_char) -> PD_PairList {
    let null_list = PD_PairList {
        data: std::ptr::null_mut(),
        len: 0,
    };
    let Some(p) = point_ref(pt) else {
        return null_list;
    };
    let Some(k) = cstr_to_str(key) else {
        return null_list;
    };
    pairs_to_ffi(p.search(k))
}

fn pairs_to_ffi(pairs: Vec<(String, String)>) -> PD_PairList {
    let mut c_pairs: Vec<PD_StringPair> = pairs
        .into_iter()
        .map(|(k, v)| PD_StringPair {
            key: string_to_c(k),
            value: string_to_c(v),
        })
        .collect();
    let len = c_pairs.len();
    c_pairs.shrink_to_fit();
    let data = c_pairs.as_mut_ptr();
    std::mem::forget(c_pairs);
    PD_PairList { data, len }
}

// ── Traversal ────────────────────────────────────────────────────────────────

/// Best-effort traversal — calls `cb(point, user_data)` for every Point.
///
/// The tree may change while `cb` runs.  `user_data` is forwarded unchanged.
#[no_mangle]
pub unsafe extern "C" fn pd_iter_lockfree(
    cb: unsafe extern "C" fn(*mut Point, *mut std::ffi::c_void),
    user_data: *mut std::ffi::c_void,
) {
    PointDexter::new().iter_lockfree(|p| {
        let handle = point_to_ptr(p.clone());
        cb(handle, user_data);
        // The callback takes ownership and must call pd_point_free.
    });
}

/// Synchronized traversal — no structural mutations during `cb`.
///
/// `cb` receives a *cloned* handle for each Point; free it with `pd_point_free`.
#[no_mangle]
pub unsafe extern "C" fn pd_iter(
    cb: unsafe extern "C" fn(*mut Point, *mut std::ffi::c_void),
    user_data: *mut std::ffi::c_void,
) {
    PointDexter::new().iter(|p| {
        let handle = point_to_ptr(p.clone());
        cb(handle, user_data);
    });
}

// ── Memory management ─────────────────────────────────────────────────────────

/// Free a `*mut c_char` string returned by any `pd_*` function.
#[no_mangle]
pub unsafe extern "C" fn pd_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// Free a `PD_StringList` and all strings it contains.
#[no_mangle]
pub unsafe extern "C" fn pd_string_list_free(list: PD_StringList) {
    if list.data.is_null() {
        return;
    }
    let slice = std::slice::from_raw_parts_mut(list.data, list.len);
    for ptr in slice.iter() {
        if !ptr.is_null() {
            drop(CString::from_raw(*ptr));
        }
    }
    drop(Box::from_raw(slice as *mut [*mut c_char]));
}

/// Free a `PD_PairList` and all strings it contains.
#[no_mangle]
pub unsafe extern "C" fn pd_pair_list_free(list: PD_PairList) {
    if list.data.is_null() {
        return;
    }
    let slice = std::slice::from_raw_parts_mut(list.data, list.len);
    for pair in slice.iter() {
        if !pair.key.is_null() {
            drop(CString::from_raw(pair.key));
        }
        if !pair.value.is_null() {
            drop(CString::from_raw(pair.value));
        }
    }
    drop(Vec::from_raw_parts(list.data, list.len, list.len));
}
