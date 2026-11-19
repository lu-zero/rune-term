//! The wire protocol between rune-term and its shell keeper.
//!
//! Rune-term hands a local shell to a [`rune_term_keeper`](../main.rs) process
//! at exit. The keeper binds the socket the next instance connects to, holds
//! the master open so the shell never sees a hangup, and serves it back on
//! that connection.

use std::io;
use std::os::fd::RawFd;

/// The control buffer for one file descriptor.
const CMSG_BUF_LEN: usize = 64;

/// Send `fd` on a connection: the receiver gets its own reference to the open
/// file description.
pub fn send_fd(sender: RawFd, fd: RawFd) -> io::Result<()> {
    let fds = [fd];
    let mut control = vec![0u8; CMSG_BUF_LEN];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    let mut none = [0u8; 1];
    let mut iov = libc::iovec {
        iov_base: none.as_mut_ptr().cast(),
        iov_len: 0,
    };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    // macOS rejects a control length beyond the message's own control data.
    msg.msg_controllen = unsafe { libc::CMSG_SPACE(4) } as _;
    let cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
    if cmsg.is_null() {
        return Err(io::Error::other("no room for the control message"));
    }
    unsafe {
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of_val(&fds) as _) as _;
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        libc::memcpy(
            libc::CMSG_DATA(cmsg).cast(),
            fds.as_ptr().cast(),
            std::mem::size_of_val(&fds),
        );
        let n = libc::sendmsg(sender, &msg, 0);
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Receive the file descriptor the other side sent, if any.
pub fn recv_master(sender: RawFd) -> Option<RawFd> {
    let mut control = vec![0u8; CMSG_BUF_LEN];
    let mut none = [0u8; 1];
    let mut iov = libc::iovec {
        iov_base: none.as_mut_ptr().cast(),
        iov_len: 0,
    };
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = control.len() as _;
    loop {
        let received = unsafe { libc::recvmsg(sender, &mut msg, 0) };
        if received < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return None;
        }
        let mut cmsg = unsafe { libc::CMSG_FIRSTHDR(&msg) };
        while !cmsg.is_null() {
            if unsafe {
                (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS
            } {
                let fd = unsafe { *libc::CMSG_DATA(cmsg).cast::<libc::c_int>() };
                return (fd >= 0).then_some(fd);
            }
            cmsg = unsafe { libc::CMSG_NXTHDR(&msg, cmsg) };
        }
        return None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_protocol_sends_and_receives_an_fd() {
        let mut sv = [0 as libc::c_int; 2];
        assert_eq!(
            unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, sv.as_mut_ptr()) },
            0
        );
        let fd = unsafe { libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY) };
        assert!(send_fd(sv[0], fd).is_ok());
        assert!(recv_master(sv[1]).is_some());
    }
}
