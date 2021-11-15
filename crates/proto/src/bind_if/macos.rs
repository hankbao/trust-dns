// macos.rs
#![cfg(target_os = "macos")]

use std::ffi::c_void;
use std::io;
use std::mem;
use std::os::unix::io::RawFd;

use libc::{setsockopt, IPPROTO_IP, IPPROTO_IPV6, IP_BOUND_IF};

// FIXME: libc doesn't define IPV6_BOUND_IF yet
const IPV6_BOUND_IF: i32 = 125; // from netinet/in.h

pub fn bind_to_if4(fd: RawFd, bind_if: u32) -> io::Result<()> {
    let if_index = bind_if;

    let ret = unsafe {
        setsockopt(
            fd,
            IPPROTO_IP,
            IP_BOUND_IF,
            &if_index as *const _ as *const c_void,
            mem::size_of_val(&if_index) as u32,
        )
    };

    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

pub fn bind_to_if6(fd: RawFd, bind_if: u32) -> io::Result<()> {
    let if_index = bind_if;

    let ret = unsafe {
        setsockopt(
            fd,
            IPPROTO_IPV6 as i32,
            IPV6_BOUND_IF,
            &if_index as *const _ as *const c_void,
            mem::size_of_val(&if_index) as u32,
        )
    };

    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
