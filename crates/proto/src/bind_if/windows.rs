// windows.rs
#![cfg(windows)]

use std::io;
use std::mem;
use std::os::windows::io::RawSocket;

use winapi::shared::ws2def::{IPPROTO_IP, IPPROTO_IPV6};
use winapi::shared::ws2ipdef::IPV6_UNICAST_IF;
use winapi::um::winsock2::{setsockopt, WSAGetLastError};

// FIXME: winapi doesn't define IP_UNICAST_IF yet
const IP_UNICAST_IF: i32 = IPV6_UNICAST_IF;

pub fn bind_to_if4(socket: RawSocket, bind_if: u32) -> io::Result<()> {
    // MSDN says for IPv4 this needs to be in net byte order,
    // so that it's like an IP address with leading zeros.
    let if_index = bind_if.to_be();

    let ret = unsafe {
        setsockopt(
            socket as usize,
            IPPROTO_IP,
            IP_UNICAST_IF,
            &if_index as *const _ as *const i8,
            mem::size_of_val(&if_index) as i32,
        )
    };

    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }))
    }
}

pub fn bind_to_if6(socket: RawSocket, bind_if: u32) -> io::Result<()> {
    let if_index = bind_if;

    let ret = unsafe {
        setsockopt(
            socket as usize,
            IPPROTO_IPV6 as i32,
            IPV6_UNICAST_IF,
            &if_index as *const _ as *const i8,
            mem::size_of_val(&if_index) as i32,
        )
    };

    if ret == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(unsafe { WSAGetLastError() }))
    }
}
