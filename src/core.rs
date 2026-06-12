#![allow(dead_code, unused, non_camel_case_types)]

use std::{
    fmt, ptr,
    sync::{
        atomic::{AtomicPtr, AtomicU64, Ordering},
        Arc, OnceLock,
    },
};

use crossbeam_skiplist::SkipMap;
use dashmap::DashMap;
use parking_lot::RwLock;

static POINTDEXTER: OnceLock<__PointDexter__> = OnceLock::new();

#[inline(always)]
fn pointdexter() -> &'static __PointDexter__ {
    POINTDEXTER.get_or_init(__PointDexter__::new)
}

struct __PointDexter__ {
    points: DashMap<String, Arc<__inner__>>,
    synchronization: RwLock<()>,
}

impl __PointDexter__ {
    fn new() -> Self {
        __PointDexter__ {
            points: DashMap::new(),
            synchronization: RwLock::new(()),
        }
    }
}

struct __inner__ {
    name: String,
    entries: SkipMap<EntryKey, String>,
    entry_sequence: AtomicU64,
    children: DashMap<String, ()>,
    parent: AtomicPtr<__inner__>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct EntryKey {
    key: String,
    sequence: u64,
}

impl __inner__ {
    fn new(name: impl Into<String>) -> Arc<Self> {
        Arc::new(__inner__ {
            name: name.into(),
            entries: SkipMap::new(),
            entry_sequence: AtomicU64::new(0),
            children: DashMap::new(),
            parent: AtomicPtr::new(ptr::null_mut()),
        })
    }

    fn insert(&self, key: impl Into<String>, value: impl Into<String>) {
        let sequence = self.entry_sequence.fetch_add(1, Ordering::Relaxed);
        self.entries.insert(
            EntryKey {
                key: key.into(),
                sequence,
            },
            value.into(),
        );
    }

    fn remove_key(&self, key: &str) {
        let to_remove: Vec<EntryKey> = self
            .entries
            .iter()
            .skip_while(|e| e.key().key.as_str() < key)
            .take_while(|e| e.key().key.as_str() == key)
            .map(|e| e.key().clone())
            .collect();

        for ek in to_remove {
            self.entries.remove(&ek);
        }
    }

    fn get(&self, key: &str) -> Vec<String> {
        let start = EntryKey {
            key: key.to_string(),
            sequence: 0,
        };
        self.entries
            .range(start..)
            .take_while(|e| e.key().key == key)
            .map(|e| e.value().clone())
            .collect()
    }

    fn all_entries(&self) -> Vec<(String, String)> {
        self.entries
            .iter()
            .map(|e| (e.key().key.clone(), e.value().clone()))
            .collect()
    }

    fn parent_pointer(&self) -> *mut __inner__ {
        self.parent.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub struct Point(Arc<__inner__>);

impl PartialEq for Point {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Point {}

impl fmt::Debug for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Point({:?})", self.0.name)
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_inner(f, &self.0, 0)
    }
}

fn fmt_inner(f: &mut fmt::Formatter<'_>, p: &__inner__, depth: usize) -> fmt::Result {
    let pad = "    ".repeat(depth);
    writeln!(f, "{}{}", pad, p.name)?;
    for e in p.entries.iter() {
        writeln!(f, "{}├── {} = {}", pad, e.key().key, e.value())?;
    }
    for c in p.children.iter() {
        if let Some(child) = pointdexter().points.get(c.key()) {
            fmt_inner(f, &child, depth + 1)?;
        }
    }
    Ok(())
}

impl Point {
    pub fn name(&self) -> &str {
        &self.0.name
    }

    pub fn insert(&self, key: impl Into<String>, value: impl Into<String>) {
        let _guard = pointdexter().synchronization.read();
        self.0.insert(key, value);
    }

    pub fn purge_key(&self, key: &str) {
        let _guard = pointdexter().synchronization.read();
        self.0.remove_key(key);
    }

    pub fn get(&self, key: &str) -> Vec<String> {
        self.0.get(key)
    }

    pub fn get_first(&self, key: &str) -> Option<String> {
        self.0.get(key).into_iter().next()
    }

    pub fn entries(&self) -> Vec<(String, String)> {
        self.0.all_entries()
    }

    pub fn attach(&self, child: &Point) -> Result<(), AttachError> {
        if Arc::ptr_eq(&self.0, &child.0) {
            return Err(AttachError::SelfAttach);
        }

        // Cycle check: walk our own ancestor chain in O(depth).
        // If we find `child` among our ancestors, attaching would point a cycle.
        if self.has_ancestor_ptr(Arc::as_ptr(&child.0)) {
            return Err(AttachError::WouldCycle);
        }

        let _guard = pointdexter().synchronization.read();

        // Atomically detach from old parent.
        let old_ptr = child.0.parent.swap(ptr::null_mut(), Ordering::AcqRel);
        if !old_ptr.is_null() {
            // SAFETY: non-null parent ptr is always kept alive by the registry Arc.
            let old_parent = unsafe { &*old_ptr };
            old_parent.children.remove(&child.0.name);
        }

        // Store new parent (raw pointer into the Arc allocation; Arc lives in registry).
        let self_ptr = Arc::as_ptr(&self.0) as *mut __inner__;
        child.0.parent.store(self_ptr, Ordering::Release);

        // Register as child — O(1) avg.
        self.0.children.insert(child.0.name.clone(), ());
        Ok(())
    }

    /// Detach this Point from its parent — O(1) amortised.
    pub fn detach(&self) {
        let _guard = pointdexter().synchronization.read();
        let old_ptr = self.0.parent.swap(ptr::null_mut(), Ordering::AcqRel);
        if !old_ptr.is_null() {
            let parent = unsafe { &*old_ptr };
            parent.children.remove(&self.0.name);
        }
    }

    /// Parent Point, if any — O(1).
    pub fn parent(&self) -> Option<Point> {
        let ptr = self.0.parent.load(Ordering::Acquire);
        if ptr.is_null() {
            return None;
        }
        let name = unsafe { &(*ptr).name };
        pointdexter()
            .points
            .get(name)
            .map(|e| Point(Arc::clone(&e)))
    }

    /// Direct children — O(C).
    pub fn children(&self) -> Vec<Point> {
        self.0
            .children
            .iter()
            .filter_map(|e| {
                pointdexter()
                    .points
                    .get(e.key())
                    .map(|p| Point(Arc::clone(&p)))
            })
            .collect()
    }

    // ── Search ───────────────────────────────────────────────────────────

    /// Scoped search: all (point_name, value) pairs for `key` within this
    /// subtree — O(N × log E) where N = subtree size, E = entries per point.
    ///
    /// Uses an explicit stack-based DFS to avoid recursion depth limits.
    pub fn search(&self, key: &str) -> Vec<(String, String)> {
        let mut results = Vec::new();
        // Explicit stack — O(depth) extra space, no stack-overflow risk.
        let mut stack: Vec<Point> = vec![self.clone()];
        while let Some(current) = stack.pop() {
            for v in current.0.get(key) {
                results.push((current.0.name.clone(), v));
            }
            stack.extend(current.children());
        }
        results
    }

    // ── Internal helpers ─────────────────────────────────────────────────

    /// Walk the ancestor chain checking for a given raw pointer — O(depth).
    fn has_ancestor_ptr(&self, target: *const __inner__) -> bool {
        let mut ptr = self.0.parent.load(Ordering::Acquire);
        while !ptr.is_null() {
            if ptr as *const __inner__ == target {
                return true;
            }
            ptr = unsafe { (*ptr).parent.load(Ordering::Acquire) };
        }
        false
    }
}

#[derive(Clone, Copy, Default)]
pub struct PointDexter;

impl PointDexter {
    pub fn new() -> Self {
        PointDexter
    }

    // points a point
    pub fn point(&self, name: impl Into<String>) -> Point {
        let name = name.into();
        // `entry().or_insert()` is atomic under DashMap's shard lock — handles
        // the race where two threads point the same name simultaneously.
        let arc = Arc::clone(
            pointdexter()
                .points
                .entry(name.clone())
                .or_insert_with(|| __inner__::new(name))
                .value(),
        );
        Point(arc)
    }

    pub fn get(&self, name: &str) -> Option<Point> {
        pointdexter()
            .points
            .get(name)
            .map(|e| Point(Arc::clone(&e)))
    }

    pub fn purge_point(&self, name: &str) {
        let _guard = pointdexter().synchronization.read();

        let Some((_, root_arc)) = pointdexter().points.remove(name) else {
            return;
        };

        // Detach from parent.
        let old_ptr = root_arc.parent.swap(ptr::null_mut(), Ordering::AcqRel);
        if !old_ptr.is_null() {
            unsafe { (*old_ptr).children.remove(name) };
        }

        // Iterative DFS subtree removal — O(N), O(depth) stack space.
        let mut stack: Vec<Arc<__inner__>> = vec![root_arc];
        while let Some(node) = stack.pop() {
            let child_names: Vec<String> = node.children.iter().map(|e| e.key().clone()).collect();
            for child_name in child_names {
                node.children.remove(&child_name);
                if let Some((_, child_arc)) = pointdexter().points.remove(&child_name) {
                    child_arc.parent.store(ptr::null_mut(), Ordering::Release);
                    stack.push(child_arc);
                }
            }
        }
    }

    pub fn search(&self, key: &str) -> Vec<(String, String)> {
        let mut results = Vec::new();
        for entry in pointdexter().points.iter() {
            let inner = entry.value();
            for v in inner.get(key) {
                results.push((inner.name.clone(), v));
            }
        }
        results
    }

    pub fn iter_lockfree<F: FnMut(&Point)>(&self, mut f: F) {
        for entry in pointdexter().points.iter() {
            f(&Point(Arc::clone(entry.value())));
        }
    }

    pub fn iter<F: FnMut(&Point)>(&self, mut f: F) {
        let _guard = pointdexter().synchronization.write();
        for entry in pointdexter().points.iter() {
            f(&Point(Arc::clone(entry.value())));
        }
    }

    pub fn roots(&self) -> Vec<Point> {
        pointdexter()
            .points
            .iter()
            .filter(|e| e.value().parent_pointer().is_null())
            .map(|e| Point(Arc::clone(e.value())))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachError {
    SelfAttach,
    WouldCycle,
}

impl fmt::Display for AttachError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttachError::SelfAttach => write!(f, "AttachError: cannot attach a Point to itself"),
            AttachError::WouldCycle => write!(f, "AttachError: attachment would point a cycle"),
        }
    }
}

impl std::error::Error for AttachError {}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // Each test uses unique Point names to avoid cross-test state pollution
    // (the global registry persists for the lifetime of the test process).

    #[test]
    fn point_idempotent() {
        let pt = PointDexter::new();
        let a = pt.point("T_Idem_A");
        let b = pt.point("T_Idem_A");
        assert!(Arc::ptr_eq(&a.0, &b.0), "same name must return same Point");
    }

    #[test]
    fn insert_and_get_first() {
        let pt = PointDexter::new();
        let u = pt.point("T_InsGet");
        u.insert("name", "John");
        u.insert("age", "24");
        assert_eq!(u.get_first("name").as_deref(), Some("John"));
        assert_eq!(u.get_first("age").as_deref(), Some("24"));
        assert_eq!(u.get_first("missing"), None);
    }

    #[test]
    fn duplicate_keys_within_point() {
        let pt = PointDexter::new();
        let p = pt.point("T_DupKey");
        p.insert("tag", "rust");
        p.insert("tag", "lock-free");
        p.insert("tag", "tree");
        let tags = p.get("tag");
        assert_eq!(tags.len(), 3);
        assert!(tags.contains(&"rust".to_owned()));
        assert!(tags.contains(&"lock-free".to_owned()));
        assert!(tags.contains(&"tree".to_owned()));
    }

    #[test]
    fn duplicate_keys_across_points_are_independent() {
        let pt = PointDexter::new();
        let u = pt.point("T_XDup_U");
        let a = pt.point("T_XDup_A");
        u.insert("role", "user");
        a.insert("role", "admin");
        assert_eq!(u.get_first("role").as_deref(), Some("user"));
        assert_eq!(a.get_first("role").as_deref(), Some("admin"));
    }

    #[test]
    fn remove_key() {
        let pt = PointDexter::new();
        let p = pt.point("T_RemKey");
        p.insert("x", "1");
        p.insert("x", "2");
        p.insert("y", "3");
        p.purge_key("x");
        assert!(p.get("x").is_empty());
        assert_eq!(p.get_first("y").as_deref(), Some("3"));
    }

    #[test]
    fn attach_and_parent_child() {
        let pt = PointDexter::new();
        let parent = pt.point("T_Att_P");
        let child = pt.point("T_Att_C");
        parent.attach(&child).unwrap();
        assert_eq!(child.parent(), Some(parent.clone()));
        assert!(parent.children().contains(&child));
    }

    #[test]
    fn detach() {
        let pt = PointDexter::new();
        let parent = pt.point("T_Det_P");
        let child = pt.point("T_Det_C");
        parent.attach(&child).unwrap();
        child.detach();
        assert!(child.parent().is_none());
        assert!(!parent.children().contains(&child));
    }

    #[test]
    fn reparent_removes_from_old_parent() {
        let pt = PointDexter::new();
        let p1 = pt.point("T_Repar_P1");
        let p2 = pt.point("T_Repar_P2");
        let c = pt.point("T_Repar_C");
        p1.attach(&c).unwrap();
        p2.attach(&c).unwrap(); // re-parents c to p2
        assert!(!p1.children().contains(&c));
        assert!(p2.children().contains(&c));
        assert_eq!(c.parent(), Some(p2.clone()));
    }

    #[test]
    fn self_attach_error() {
        let pt = PointDexter::new();
        let a = pt.point("T_SelfAtt");
        assert_eq!(a.attach(&a), Err(AttachError::SelfAttach));
    }

    #[test]
    fn cycle_detection() {
        let pt = PointDexter::new();
        let a = pt.point("T_Cycle_A");
        let b = pt.point("T_Cycle_B");
        let c = pt.point("T_Cycle_C");
        a.attach(&b).unwrap();
        b.attach(&c).unwrap();
        // Attaching a under c would make c an ancestor of itself.
        assert_eq!(c.attach(&a), Err(AttachError::WouldCycle));
        // Tree structure must be unchanged.
        assert_eq!(b.parent(), Some(a.clone()));
        assert_eq!(c.parent(), Some(b.clone()));
    }

    #[test]
    fn stable_identity_after_reparent() {
        let pt = PointDexter::new();
        let p1 = pt.point("T_StableId_P1");
        let p2 = pt.point("T_StableId_P2");
        let c = pt.point("T_StableId_C");
        c.insert("city", "Bangalore");
        p1.attach(&c).unwrap();
        let ptr_before = Arc::as_ptr(&c.0);
        p2.attach(&c).unwrap();
        assert_eq!(
            Arc::as_ptr(&c.0),
            ptr_before,
            "Arc allocation must not change"
        );
        assert_eq!(c.get_first("city").as_deref(), Some("Bangalore"));
    }

    #[test]
    fn global_search_finds_all_points() {
        let pt = PointDexter::new();
        let u = pt.point("T_GSrch_U");
        let a = pt.point("T_GSrch_A");
        u.insert("email", "john@example.com");
        a.insert("email", "alice@example.com");
        let results = pt.search("email");
        let values: Vec<_> = results.iter().map(|(_, v)| v.as_str()).collect();
        assert!(values.contains(&"john@example.com"));
        assert!(values.contains(&"alice@example.com"));
    }

    #[test]
    fn scoped_search_only_subtree() {
        let pt = PointDexter::new();
        let u = pt.point("T_SSrch_U");
        let a = pt.point("T_SSrch_A");
        u.insert("name", "John");
        a.insert("name", "Alice");
        let results = u.search("name");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].1, "John");
    }

    #[test]
    fn scoped_search_includes_all_descendants() {
        let pt = PointDexter::new();
        let root = pt.point("T_Deep_R");
        let child = pt.point("T_Deep_C");
        let grandch = pt.point("T_Deep_G");
        root.attach(&child).unwrap();
        child.attach(&grandch).unwrap();
        root.insert("x", "r");
        child.insert("x", "c");
        grandch.insert("x", "g");
        let results = root.search("x");
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn direct_lookup_via_tree_get() {
        let pt = PointDexter::new();
        let u = pt.point("T_DLookup");
        u.insert("age", "24");
        let age = pt.get("T_DLookup").and_then(|p| p.get_first("age"));
        assert_eq!(age.as_deref(), Some("24"));
    }

    #[test]
    fn delete_key_leaves_others() {
        let pt = PointDexter::new();
        let p = pt.point("T_DelKey");
        p.insert("name", "John");
        p.insert("age", "24");
        p.purge_key("age");
        assert!(p.get("age").is_empty());
        assert_eq!(p.get_first("name").as_deref(), Some("John"));
    }

    #[test]
    fn delete_point_removes_entire_subtree() {
        let pt = PointDexter::new();
        let u = pt.point("T_DelPt_U");
        let pr = pt.point("T_DelPt_Pr");
        let s = pt.point("T_DelPt_S");
        u.attach(&pr).unwrap();
        pr.attach(&s).unwrap();
        s.insert("theme", "Dark");

        pt.purge_point("T_DelPt_Pr");

        assert!(pt.get("T_DelPt_Pr").is_none(), "Profile must be gone");
        assert!(pt.get("T_DelPt_S").is_none(), "Settings must be gone");
        assert!(u.children().is_empty(), "Users must have no children");
    }

    #[test]
    fn delete_root_of_deep_subtree() {
        let pt = PointDexter::new();
        let names: Vec<String> = (0..50).map(|i| format!("T_Deep50_{}", i)).collect();
        let points: Vec<Point> = names.iter().map(|n| pt.point(n.as_str())).collect();
        for i in 1..50 {
            points[i - 1].attach(&points[i]).unwrap();
        }
        pt.purge_point(&names[0]);
        for n in &names {
            assert!(pt.get(n).is_none(), "{n} should be deleted");
        }
    }

    #[test]
    fn roots_returns_only_parentless_points() {
        let pt = PointDexter::new();
        let r = pt.point("T_Roots_R");
        let c = pt.point("T_Roots_C");
        r.attach(&c).unwrap();
        let roots = pt.roots();
        assert!(roots.contains(&r));
        assert!(!roots.contains(&c));
    }

    #[test]
    fn best_effort_traversal_visits_all() {
        let pt = PointDexter::new();
        pt.point("T_BETrav_A");
        pt.point("T_BETrav_B");
        let mut seen = Vec::new();
        pt.iter_lockfree(|p| seen.push(p.name().to_owned()));
        assert!(seen.contains(&"T_BETrav_A".to_owned()));
        assert!(seen.contains(&"T_BETrav_B".to_owned()));
    }

    #[test]
    fn sync_traversal_visits_all() {
        let pt = PointDexter::new();
        pt.point("T_SyncTr_A");
        pt.point("T_SyncTr_B");
        let mut seen = Vec::new();
        pt.iter(|p| seen.push(p.name().to_owned()));
        assert!(seen.contains(&"T_SyncTr_A".to_owned()));
        assert!(seen.contains(&"T_SyncTr_B".to_owned()));
    }

    #[test]
    fn concurrent_points_are_idempotent() {
        use std::thread;
        let handles: Vec<_> = (0..16)
            .map(|_| {
                thread::spawn(|| {
                    let pt = PointDexter::new();
                    let p = pt.point("T_Concpoint");
                    Arc::as_ptr(&p.0) as usize
                })
            })
            .collect();
        let ptrs: Vec<usize> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(
            ptrs.windows(2).all(|w| w[0] == w[1]),
            "all threads must see the same Arc allocation"
        );
    }

    #[test]
    fn concurrent_inserts_all_recorded() {
        use std::thread;
        let _ = PointDexter::new().point("T_ConcIns");
        let handles: Vec<_> = (0..32_u64)
            .map(|i| {
                thread::spawn(move || {
                    PointDexter::new()
                        .point("T_ConcIns")
                        .insert("n", i.to_string());
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let count = PointDexter::new().get("T_ConcIns").unwrap().get("n").len();
        assert_eq!(count, 32);
    }

    #[test]
    fn concurrent_attach_detach() {
        use std::thread;
        let pt = PointDexter::new();
        let parent = pt.point("T_ConcAtt_P");
        let child = pt.point("T_ConcAtt_C");
        parent.attach(&child).unwrap();

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let p = parent.clone();
                let c = child.clone();
                thread::spawn(move || {
                    if i % 2 == 0 {
                        let _ = p.attach(&c);
                    } else {
                        c.detach();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        // After all operations the parent/child link must be internally consistent.
        let c_has_parent = child.parent().is_some();
        let p_has_child = parent.children().contains(&child);
        assert_eq!(
            c_has_parent, p_has_child,
            "parent link and child set must agree"
        );
    }
}
