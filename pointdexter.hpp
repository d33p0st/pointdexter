/**
 * pointdexter.hpp — Zero-overhead C++ RAII wrapper around the Pointdexter C FFI.
 *
 * Header-only. Include this instead of (or after) pointdexter.h.
 * Every object is a thin wrapper around the opaque C handle — no virtual
 * dispatch, no heap overhead beyond what the Rust library itself allocates.
 *
 * Build & link:
 *   g++ -std=c++17 -O2 -o myapp myapp.cpp \
 *       -I. -L. -lpointdexter -Wl,-rpath,.
 */

#pragma once

#include "pointdexter.h"

#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace PointDexter {

// ── Forward declarations ──────────────────────────────────────────────────────
class Point;

// ── Attach errors ─────────────────────────────────────────────────────────────

class AttachError : public std::runtime_error {
public:
    explicit AttachError(const char *msg) : std::runtime_error(msg) {}
};

// ── RAII string helper ────────────────────────────────────────────────────────

class RustString {
    char *ptr_;
public:
    explicit RustString(char *p) noexcept : ptr_(p) {}
    ~RustString() { pd_string_free(ptr_); }
    RustString(const RustString &)            = delete;
    RustString &operator=(const RustString &) = delete;
    RustString(RustString &&o) noexcept : ptr_(std::exchange(o.ptr_, nullptr)) {}

    const char *c_str() const noexcept { return ptr_ ? ptr_ : ""; }
    std::string str()   const          { return ptr_ ? ptr_ : ""; }
    bool        empty() const noexcept { return !ptr_ || ptr_[0] == '\0'; }
};

// ── Point ─────────────────────────────────────────────────────────────────────

/**
 * RAII handle to a Pointdexter Point.
 *
 * Movable, non-copyable (use clone() to get a second handle to the same
 * logical Point).  All operations are lock-free except iter.
 */
class Point {
    PD_Point *raw_;

    explicit Point(PD_Point *raw) noexcept : raw_(raw) {}

    friend class PointDexter;

public:
    // ── Construction / destruction ────────────────────────────────────────────

    Point(Point &&o) noexcept : raw_(std::exchange(o.raw_, nullptr)) {}
    Point &operator=(Point &&o) noexcept {
        if (this != &o) { pd_point_free(raw_); raw_ = std::exchange(o.raw_, nullptr); }
        return *this;
    }

    Point(const Point &)            = delete;
    Point &operator=(const Point &) = delete;

    ~Point() { pd_point_free(raw_); }

    /// Clone — both handles refer to the same underlying Point.
    Point clone() const {
        PD_Point *c = pd_point_clone(raw_);
        if (!c) throw std::runtime_error("pd_point_clone failed");
        return Point(c);
    }

    bool valid() const noexcept { return raw_ != nullptr; }

    // ── Identity ──────────────────────────────────────────────────────────────

    std::string name() const {
        return RustString(pd_name(raw_)).str();
    }

    // ── Entries ───────────────────────────────────────────────────────────────

    /// Insert a key/value entry — O(log E). Duplicate keys are permitted.
    void insert(std::string_view key, std::string_view value) {
        pd_insert(raw_,
                  std::string(key).c_str(),
                  std::string(value).c_str());
    }

    /// Remove all entries with the given key — O(log E + k).
    void purge_key(std::string_view key) {
        pd_purge_key(raw_, std::string(key).c_str());
    }

    /// All values for a key — O(log E + k).
    std::vector<std::string> get(std::string_view key) const {
        PD_StringList sl = pd_get_values(raw_, std::string(key).c_str());
        std::vector<std::string> out;
        out.reserve(sl.len);
        for (size_t i = 0; i < sl.len; ++i)
            out.emplace_back(sl.data[i]);
        pd_string_list_free(sl);
        return out;
    }

    /// First value for a key, or std::nullopt if absent — O(log E).
    std::optional<std::string> get_first(std::string_view key) const {
        char *v = pd_get_first(raw_, std::string(key).c_str());
        if (!v) return std::nullopt;
        std::string s(v);
        pd_string_free(v);
        return s;
    }

    // ── Relationships ─────────────────────────────────────────────────────────

    /**
     * Attach child under this Point.
     * Throws AttachError on self-attach or cycle.
     * Re-parents child atomically if it already has a parent.
     */
    void attach(Point &child) {
        int rc = pd_attach(raw_, child.raw_);
        if (rc == PD_ERR_SELF)  throw AttachError("cannot attach a point to itself");
        if (rc == PD_ERR_CYCLE) throw AttachError("attachment would create a cycle");
    }

    /// Detach this Point from its parent — O(1).
    void detach() { pd_detach(raw_); }

    /// Return the parent, or std::nullopt if this is a root — O(1).
    std::optional<Point> parent() const {
        PD_Point *p = pd_parent(raw_);
        if (!p) return std::nullopt;
        return Point(p);
    }

    /// Direct children — O(C).
    std::vector<Point> children() const {
        size_t len = 0;
        PD_Point **arr = pd_children(raw_, &len);
        std::vector<Point> out;
        out.reserve(len);
        for (size_t i = 0; i < len; ++i)
            out.push_back(Point(arr[i]));
        pd_point_array_free(arr, len);
        return out;
    }

    // ── Search ────────────────────────────────────────────────────────────────

    /// Scoped search within this subtree — O(N × log E).
    std::vector<std::pair<std::string, std::string>>
    search(std::string_view key) const {
        PD_PairList pl = pd_search(raw_, std::string(key).c_str());
        std::vector<std::pair<std::string, std::string>> out;
        out.reserve(pl.len);
        for (size_t i = 0; i < pl.len; ++i)
            out.emplace_back(pl.data[i].key, pl.data[i].value);
        pd_pair_list_free(pl);
        return out;
    }
};

// ── PointDexter (entry point) ─────────────────────────────────────────────────

/**
 * Entry point for all Pointdexter operations.
 *
 * Stateless — all state lives in the process-global Rust registry.
 * Multiple PointDexter instances share the same underlying tree.
 */
class PointDexter {
public:
    PointDexter() = default;

    // ── Point lifecycle ───────────────────────────────────────────────────────

    /// Create or retrieve a Point by name — O(1) amortized.
    Point point(std::string_view name) {
        PD_Point *p = pd_point(std::string(name).c_str());
        if (!p) throw std::runtime_error("pd_point failed");
        return Point(p);
    }

    /// Retrieve an existing Point, or std::nullopt — O(1) amortized.
    std::optional<Point> get(std::string_view name) {
        PD_Point *p = pd_get(std::string(name).c_str());
        if (!p) return std::nullopt;
        return Point(p);
    }

    /// Delete a Point and its entire subtree — O(N).
    void purge_point(std::string_view name) {
        pd_purge_point(std::string(name).c_str());
    }

    // ── Search ────────────────────────────────────────────────────────────────

    /// Global search across all Points — O(P × log E).
    std::vector<std::pair<std::string, std::string>>
    search(std::string_view key) {
        PD_PairList pl = pd_search_global(std::string(key).c_str());
        std::vector<std::pair<std::string, std::string>> out;
        out.reserve(pl.len);
        for (size_t i = 0; i < pl.len; ++i)
            out.emplace_back(pl.data[i].key, pl.data[i].value);
        pd_pair_list_free(pl);
        return out;
    }

    // ── Traversal ─────────────────────────────────────────────────────────────

    /// Best-effort traversal — lambda receives each Point by move.
    template<typename F>
    void iter_lockfree(F &&f) {
        auto wrapper = [](PD_Point *raw, void *ud) {
            Point p(raw);
            (*reinterpret_cast<F*>(ud))(std::move(p));
        };
        pd_iter_lockfree(wrapper, reinterpret_cast<void*>(&f));
    }

    /// Synchronized traversal — no structural mutations during f.
    template<typename F>
    void iter(F &&f) {
        auto wrapper = [](PD_Point *raw, void *ud) {
            Point p(raw);
            (*reinterpret_cast<F*>(ud))(std::move(p));
        };
        pd_iter(wrapper, reinterpret_cast<void*>(&f));
    }
};

} // namespace PointDexter
