"""
pointdexter.py — Pointdexter from Python via ctypes.

Calls the compiled Rust shared library directly — zero Python extension
overhead; operations run at native Rust speed inside the library.

Usage:
    from pointdexter import PointDexter, Point, AttachError

    pd = PointDexter()
    users = pd.point("Users")
    users.insert("name", "John")
    print(users.get_first("name"))   # "John"
"""

import ctypes
import os
import sys
from pathlib import Path
from typing import Optional

# ── Locate the shared library ─────────────────────────────────────────────────

def _find_lib() -> ctypes.CDLL:
    """Search for libpointdexter next to this file, then common build paths."""
    here = Path(__file__).parent
    candidates = [
        # same directory as this .py (release archive layout)
        here / "libpointdexter.so",
        here / "libpointdexter.dylib",
        here / "pointdexter.dll",
        # cargo build --release output (development layout)
        here.parent / "target" / "release" / "libpointdexter.so",
        here.parent / "target" / "release" / "libpointdexter.dylib",
        here.parent / "target" / "release" / "pointdexter.dll",
        Path("target/release/libpointdexter.so"),
        Path("target/release/libpointdexter.dylib"),
        Path("target/release/pointdexter.dll"),
    ]
    for p in candidates:
        if p.exists():
            return ctypes.CDLL(str(p))
    raise FileNotFoundError(
        "libpointdexter not found. Either place it next to this file or run: "
        "cargo build --release"
    )

_lib = _find_lib()

# ── C struct mirrors ──────────────────────────────────────────────────────────

class PD_StringList(ctypes.Structure):
    """PD_StringList { char **data; size_t len; }"""
    _fields_ = [
        ("data", ctypes.POINTER(ctypes.c_char_p)),
        ("len",  ctypes.c_size_t),
    ]

class PD_StringPair(ctypes.Structure):
    """{ char *key; char *value; }"""
    _fields_ = [
        ("key",   ctypes.c_char_p),
        ("value", ctypes.c_char_p),
    ]

class PD_PairList(ctypes.Structure):
    """PD_PairList { PD_StringPair *data; size_t len; }"""
    _fields_ = [
        ("data", ctypes.POINTER(PD_StringPair)),
        ("len",  ctypes.c_size_t),
    ]

# Opaque handle type (void * at the C ABI level)
_PD_Point_p = ctypes.c_void_p

# ── Declare function signatures ───────────────────────────────────────────────
# Explicit argtypes/restype prevents ctypes from silently truncating 64-bit
# pointers on some platforms and gives meaningful AttributeError messages.

def _sig(name, restype, *argtypes):
    fn = getattr(_lib, name)
    fn.restype  = restype
    fn.argtypes = list(argtypes)
    return fn

_pd_point               = _sig("pd_point",               _PD_Point_p,   ctypes.c_char_p)
_pd_get                  = _sig("pd_get",                  _PD_Point_p,   ctypes.c_char_p)
_pd_point_clone          = _sig("pd_point_clone",          _PD_Point_p,   _PD_Point_p)
_pd_point_free           = _sig("pd_point_free",           None,          _PD_Point_p)
_pd_purge_point         = _sig("pd_purge_point",         ctypes.c_int,  ctypes.c_char_p)
_pd_insert               = _sig("pd_insert",               ctypes.c_int,  _PD_Point_p, ctypes.c_char_p, ctypes.c_char_p)
_pd_purge_key           = _sig("pd_purge_key",           ctypes.c_int,  _PD_Point_p, ctypes.c_char_p)
_pd_get_values           = _sig("pd_get_values",           PD_StringList, _PD_Point_p, ctypes.c_char_p)
# c_void_p (not c_char_p) for Rust-allocated strings: ctypes does NOT auto-free
# c_void_p, so we control the lifetime and call pd_string_free ourselves.
_pd_get_first            = _sig("pd_get_first",            ctypes.c_void_p, _PD_Point_p, ctypes.c_char_p)
_pd_name                 = _sig("pd_name",                 ctypes.c_void_p, _PD_Point_p)
_pd_attach               = _sig("pd_attach",               ctypes.c_int,  _PD_Point_p, _PD_Point_p)
_pd_detach               = _sig("pd_detach",               ctypes.c_int,  _PD_Point_p)
_pd_parent               = _sig("pd_parent",               _PD_Point_p,   _PD_Point_p)
_pd_children             = _sig("pd_children",             ctypes.POINTER(_PD_Point_p), _PD_Point_p, ctypes.POINTER(ctypes.c_size_t))
_pd_point_array_free     = _sig("pd_point_array_free",     None,          ctypes.POINTER(_PD_Point_p), ctypes.c_size_t)
_pd_search_global        = _sig("pd_search_global",        PD_PairList,   ctypes.c_char_p)
_pd_search               = _sig("pd_search",               PD_PairList,   _PD_Point_p, ctypes.c_char_p)
_pd_iter_lockfree = _sig("pd_iter_lockfree", None,          ctypes.CFUNCTYPE(None, _PD_Point_p, ctypes.c_void_p), ctypes.c_void_p)
_pd_iter        = _sig("pd_iter",        None,          ctypes.CFUNCTYPE(None, _PD_Point_p, ctypes.c_void_p), ctypes.c_void_p)
_pd_string_free          = _sig("pd_string_free",          None,          ctypes.c_void_p)
_pd_string_list_free     = _sig("pd_string_list_free",     None,          PD_StringList)
_pd_pair_list_free       = _sig("pd_pair_list_free",       None,          PD_PairList)

# ── Public error-code constants ───────────────────────────────────────────────

PD_OK        = 0
PD_ERR_NULL  = 1
PD_ERR_SELF  = 2
PD_ERR_CYCLE = 3
PD_ERR_UTF8  = 4

# ── Internal helpers ──────────────────────────────────────────────────────────

def _enc(s: str) -> bytes:
    return s.encode("utf-8")

def _rust_string(ptr: int) -> Optional[str]:
    """
    Decode a Rust-allocated char* (returned as a c_void_p integer) and free it.
    Returns None on NULL.  Never pass a ctypes.c_char_p here — that type
    auto-decodes but does NOT free, causing a leak; c_void_p is intentional.
    """
    if not ptr:
        return None
    s = ctypes.cast(ptr, ctypes.c_char_p).value  # borrow, read bytes
    _pd_string_free(ctypes.c_void_p(ptr))         # free via Rust allocator
    return s.decode("utf-8") if s else ""

# ── Public exception ──────────────────────────────────────────────────────────

class AttachError(Exception):
    """Raised when pd_attach returns PD_ERR_SELF or PD_ERR_CYCLE."""

# ── Point ─────────────────────────────────────────────────────────────────────

class Point:
    """
    RAII handle to a Pointdexter Point.

    The underlying C handle is freed automatically when this object is
    garbage-collected.  Use .clone() to get a second handle to the same
    logical Point.

    All data operations are lock-free and run at full native Rust speed.
    """

    __slots__ = ("_raw",)

    def __init__(self, raw):
        if not raw:
            raise ValueError("NULL Point handle")
        self._raw = raw

    def __del__(self):
        if self._raw:
            _pd_point_free(self._raw)
            self._raw = None

    def clone(self) -> "Point":
        """Return a second handle to the same underlying Point."""
        return Point(_pd_point_clone(self._raw))

    # ── Identity ─────────────────────────────────────────────────────────────

    @property
    def name(self) -> str:
        return _rust_string(_pd_name(self._raw)) or ""

    def __repr__(self) -> str:
        return f"Point({self.name!r})"

    # ── Entries ──────────────────────────────────────────────────────────────

    def insert(self, key: str, value: str) -> None:
        """Insert a key/value entry — O(log E). Duplicate keys are allowed."""
        _pd_insert(self._raw, _enc(key), _enc(value))

    def purge_key(self, key: str) -> None:
        """Remove all entries with the given key — O(log E + k)."""
        _pd_purge_key(self._raw, _enc(key))

    def get(self, key: str) -> list[str]:
        """All values for a key, in insertion order — O(log E + k)."""
        sl = _pd_get_values(self._raw, _enc(key))
        results = [sl.data[i].decode("utf-8") for i in range(sl.len)]
        _pd_string_list_free(sl)
        return results

    def get_first(self, key: str) -> Optional[str]:
        """First value for a key, or None — O(log E)."""
        return _rust_string(_pd_get_first(self._raw, _enc(key)))

    # ── Relationships ─────────────────────────────────────────────────────────

    def attach(self, child: "Point") -> None:
        """
        Attach child under this Point.
        Raises AttachError on self-attach or cycle.
        Re-parents child atomically if it already has a parent.
        """
        rc = _pd_attach(self._raw, child._raw)
        if rc == PD_ERR_SELF:  raise AttachError("cannot attach a point to itself")
        if rc == PD_ERR_CYCLE: raise AttachError("attachment would create a cycle")

    def detach(self) -> None:
        """Detach from parent — O(1). No-op if already a root."""
        _pd_detach(self._raw)

    @property
    def parent(self) -> Optional["Point"]:
        """Parent Point, or None if this is a root — O(1)."""
        raw = _pd_parent(self._raw)
        return Point(raw) if raw else None

    @property
    def children(self) -> list["Point"]:
        """Direct children — O(C)."""
        length = ctypes.c_size_t(0)
        arr = _pd_children(self._raw, ctypes.byref(length))
        n = length.value
        result = [Point(arr[i]) for i in range(n)]
        # Free the array container only — Point.__init__ now owns each handle.
        _pd_point_array_free(arr, n)
        return result

    # ── Search ────────────────────────────────────────────────────────────────

    def search(self, key: str) -> list[tuple[str, str]]:
        """Scoped search within this subtree — O(N × log E)."""
        pl = _pd_search(self._raw, _enc(key))
        results = [(pl.data[i].key.decode(), pl.data[i].value.decode())
                   for i in range(pl.len)]
        _pd_pair_list_free(pl)
        return results

# ── PointDexter ───────────────────────────────────────────────────────────────

class PointDexter:
    """
    Entry point for all Pointdexter operations.

    Stateless — all state lives in the process-global Rust registry.
    Multiple PointDexter instances share the same underlying tree.
    """

    # ── Point lifecycle ───────────────────────────────────────────────────────

    def point(self, name: str) -> Point:
        """Create or retrieve a Point by name — O(1) amortized."""
        return Point(_pd_point(_enc(name)))

    def get(self, name: str) -> Optional[Point]:
        """Retrieve an existing Point, or None if not found — O(1) amortized."""
        raw = _pd_get(_enc(name))
        return Point(raw) if raw else None

    def purge_point(self, name: str) -> None:
        """Delete a Point and its entire subtree from the registry — O(N)."""
        _pd_purge_point(_enc(name))

    # ── Search ────────────────────────────────────────────────────────────────

    def search(self, key: str) -> list[tuple[str, str]]:
        """Global search across all Points — O(P × log E)."""
        pl = _pd_search_global(_enc(key))
        results = [(pl.data[i].key.decode(), pl.data[i].value.decode())
                   for i in range(pl.len)]
        _pd_pair_list_free(pl)
        return results

    # ── Traversal ─────────────────────────────────────────────────────────────

    def iter_lockfree(self, callback) -> None:
        """
        Best-effort traversal — calls callback(Point) for every Point.
        The tree may change while callback runs.  Each Point handle is owned
        by the callback and freed when the Point object is GC'd.
        """
        @ctypes.CFUNCTYPE(None, _PD_Point_p, ctypes.c_void_p)
        def _cb(raw, _ud):
            callback(Point(raw))

        _pd_iter_lockfree(_cb, None)

    def iter(self, callback) -> None:
        """Synchronized traversal — no structural mutations during callback."""
        @ctypes.CFUNCTYPE(None, _PD_Point_p, ctypes.c_void_p)
        def _cb(raw, _ud):
            callback(Point(raw))

        _pd_iter(_cb, None)

