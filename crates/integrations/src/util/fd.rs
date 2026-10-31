//! Readiness of raw descriptors, for event loops that share a socket with a
//! library which does its own reading.

use std::{io, os::fd::AsFd};

use rustix::event::{PollFd, PollFlags, Timespec, poll};

/// Whether a read on `fd` would return without waiting: data, a hang-up or
/// an error all count, since each is something the reader must see.
pub fn has_input(fd: impl AsFd) -> io::Result<bool> {
    let mut fds = [PollFd::new(&fd, PollFlags::IN)];
    let immediately = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    Ok(poll(&mut fds, Some(&immediately))? > 0)
}
