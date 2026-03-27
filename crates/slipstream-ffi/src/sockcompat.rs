//! Platform-agnostic socket type aliases.
//!
//! On Unix, the `libc` crate provides `sockaddr_storage`, `sockaddr_in`, etc.
//! On Windows MSVC, `libc` only provides a handful of types, so we define the
//! Windows socket structures here to match the Windows SDK layout.

// ── Unix: re-export from libc ──────────────────────────────────────────────
#[cfg(not(windows))]
pub use libc::{
    in6_addr, in_addr, sa_family_t, sockaddr_in, sockaddr_in6, sockaddr_storage, AF_INET, AF_INET6,
};

// ── Windows: hand-rolled definitions matching the Windows SDK ──────────────
#[cfg(windows)]
pub use self::win::*;

#[cfg(windows)]
mod win {
    use libc::c_int;

    pub const AF_INET: c_int = 2;
    pub const AF_INET6: c_int = 23;

    pub type sa_family_t = u16;

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct in_addr {
        pub s_addr: u32,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct sockaddr_in {
        pub sin_family: sa_family_t,
        pub sin_port: u16,
        pub sin_addr: in_addr,
        pub sin_zero: [u8; 8],
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct in6_addr {
        pub s6_addr: [u8; 16],
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct sockaddr_in6 {
        pub sin6_family: sa_family_t,
        pub sin6_port: u16,
        pub sin6_flowinfo: u32,
        pub sin6_addr: in6_addr,
        pub sin6_scope_id: u32,
    }

    /// Windows `SOCKADDR_STORAGE` — 128 bytes, 8-byte aligned.
    #[repr(C, align(8))]
    #[derive(Copy, Clone)]
    pub struct sockaddr_storage {
        pub ss_family: sa_family_t,
        pub __ss_pad1: [u8; 6],
        pub __ss_align: i64,
        pub __ss_pad2: [u8; 112],
    }
}
