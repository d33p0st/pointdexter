mod core;
mod ffi;
pub mod prelude;

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    pub use crate::core::tests::*;

    use crate::ffi::*;
    use std::ffi::{CStr, CString};

    // Binds a CString to a named local variable, returning its raw pointer.
    // Necessary because CString::new("x").as_ptr() has the CString drop
    // immediately after the statement; binding it keeps it alive for the block.
    macro_rules! cstr {
        ($name:ident, $s:expr) => {
            let $name = CString::new($s).unwrap();
            let $name = $name.as_ptr();
        };
    }

    #[test]
    fn ffi_create_and_name() {
        cstr!(name, "FFI_Create");
        let pt = unsafe { pd_point(name) };
        assert!(!pt.is_null());
        let got = unsafe { pd_name(pt) };
        let s = unsafe { CStr::from_ptr(got).to_str().unwrap() };
        assert_eq!(s, "FFI_Create");
        unsafe {
            pd_string_free(got);
            pd_point_free(pt);
        }
    }

    #[test]
    fn ffi_insert_and_get_first() {
        cstr!(n, "FFI_Ins");
        cstr!(k, "city");
        cstr!(v, "Bangalore");
        let pt = unsafe { pd_point(n) };
        assert_eq!(unsafe { pd_insert(pt, k, v) }, PD_OK);
        cstr!(k2, "city");
        let val = unsafe { pd_get_first(pt, k2) };
        assert!(!val.is_null());
        assert_eq!(
            unsafe { CStr::from_ptr(val).to_str().unwrap() },
            "Bangalore"
        );
        unsafe {
            pd_string_free(val);
            pd_point_free(pt);
        }
    }

    #[test]
    fn ffi_insert_get_values_multi() {
        cstr!(n, "FFI_Multi");
        let pt = unsafe { pd_point(n) };
        for s in ["a", "b", "c"] {
            let tag = CString::new("tag").unwrap();
            let val = CString::new(s).unwrap();
            unsafe {
                pd_insert(pt, tag.as_ptr(), val.as_ptr());
            }
        }
        cstr!(k, "tag");
        let list = unsafe { pd_get_values(pt, k) };
        assert_eq!(list.len, 3);
        unsafe {
            pd_string_list_free(list);
            pd_point_free(pt);
        }
    }

    #[test]
    fn ffi_attach_detach() {
        cstr!(pn, "FFI_Par");
        cstr!(cn, "FFI_Chi");
        let par = unsafe { pd_point(pn) };
        let chi = unsafe { pd_point(cn) };
        assert_eq!(unsafe { pd_attach(par, chi) }, PD_OK);
        let ph = unsafe { pd_parent(chi) };
        assert!(!ph.is_null());
        unsafe {
            pd_point_free(ph);
            pd_detach(chi);
        }
        assert!(unsafe { pd_parent(chi) }.is_null());
        unsafe {
            pd_point_free(par);
            pd_point_free(chi);
        }
    }

    #[test]
    fn ffi_attach_self_error() {
        cstr!(n, "FFI_SelfAtt");
        let p = unsafe { pd_point(n) };
        assert_eq!(unsafe { pd_attach(p, p) }, PD_ERR_SELF);
        unsafe {
            pd_point_free(p);
        }
    }

    #[test]
    fn ffi_attach_cycle_error() {
        cstr!(na, "FFI_CycA2");
        cstr!(nb, "FFI_CycB2");
        cstr!(nc, "FFI_CycC2");
        let a = unsafe { pd_point(na) };
        let b = unsafe { pd_point(nb) };
        let c = unsafe { pd_point(nc) };
        unsafe {
            pd_attach(a, b);
            pd_attach(b, c);
        }
        assert_eq!(unsafe { pd_attach(c, a) }, PD_ERR_CYCLE);
        unsafe {
            pd_point_free(a);
            pd_point_free(b);
            pd_point_free(c);
        }
    }

    #[test]
    fn ffi_global_search() {
        cstr!(nu, "FFI_GSU2");
        cstr!(na, "FFI_GSA2");
        let u = unsafe { pd_point(nu) };
        let a = unsafe { pd_point(na) };
        let lang = CString::new("lang").unwrap();
        let rust = CString::new("Rust").unwrap();
        let cpp = CString::new("C++").unwrap();
        unsafe {
            pd_insert(u, lang.as_ptr(), rust.as_ptr());
            pd_insert(a, lang.as_ptr(), cpp.as_ptr());
        }
        let results = unsafe { pd_search_global(lang.as_ptr()) };
        assert!(results.len >= 2);
        unsafe {
            pd_pair_list_free(results);
            pd_point_free(u);
            pd_point_free(a);
        }
    }

    #[test]
    fn ffi_scoped_search() {
        cstr!(rn, "FFI_SRoot2");
        cstr!(cn, "FFI_SChi2");
        let root = unsafe { pd_point(rn) };
        let child = unsafe { pd_point(cn) };
        unsafe {
            pd_attach(root, child);
        }
        let x = CString::new("x").unwrap();
        let rv = CString::new("root_val").unwrap();
        let cv = CString::new("child_val").unwrap();
        unsafe {
            pd_insert(root, x.as_ptr(), rv.as_ptr());
            pd_insert(child, x.as_ptr(), cv.as_ptr());
        }
        let results = unsafe { pd_search(root, x.as_ptr()) };
        assert_eq!(results.len, 2);
        unsafe {
            pd_pair_list_free(results);
            pd_point_free(root);
            pd_point_free(child);
        }
    }

    #[test]
    fn ffi_children_list() {
        cstr!(pn, "FFI_ChlPar2");
        cstr!(c1n, "FFI_Chl12");
        cstr!(c2n, "FFI_Chl22");
        let par = unsafe { pd_point(pn) };
        let c1 = unsafe { pd_point(c1n) };
        let c2 = unsafe { pd_point(c2n) };
        unsafe {
            pd_attach(par, c1);
            pd_attach(par, c2);
        }
        let mut len: usize = 0;
        let arr = unsafe { pd_children(par, &mut len) };
        assert_eq!(len, 2);
        for i in 0..len {
            unsafe {
                pd_point_free(*arr.add(i));
            }
        }
        unsafe {
            pd_point_array_free(arr, len);
        }
        unsafe {
            pd_point_free(par);
            pd_point_free(c1);
            pd_point_free(c2);
        }
    }

    #[test]
    fn ffi_delete_point() {
        cstr!(n, "FFI_Del2");
        cstr!(k, "k");
        cstr!(v, "v");
        let p = unsafe { pd_point(n) };
        unsafe {
            pd_insert(p, k, v);
        }
        cstr!(n2, "FFI_Del2");
        assert_eq!(unsafe { pd_purge_point(n2) }, PD_OK);
        cstr!(n3, "FFI_Del2");
        let lookup = unsafe { pd_get(n3) };
        assert!(lookup.is_null());
        unsafe {
            pd_point_free(p);
        }
    }

    #[test]
    fn ffi_null_safety() {
        unsafe {
            assert!(pd_point(std::ptr::null()).is_null());
            assert!(pd_get(std::ptr::null()).is_null());
            assert!(pd_get_first(std::ptr::null_mut(), std::ptr::null()).is_null());
            assert_eq!(
                pd_insert(std::ptr::null_mut(), std::ptr::null(), std::ptr::null()),
                PD_ERR_NULL
            );
            assert_eq!(
                pd_attach(std::ptr::null_mut(), std::ptr::null_mut()),
                PD_ERR_NULL
            );
            assert_eq!(pd_detach(std::ptr::null_mut()), PD_ERR_NULL);
            pd_point_free(std::ptr::null_mut());
            pd_string_free(std::ptr::null_mut());
        }
    }
}
