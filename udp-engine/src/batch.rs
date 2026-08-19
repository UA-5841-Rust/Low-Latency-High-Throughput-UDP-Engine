use libc::{iovec, mmsghdr, msghdr, sockaddr_storage, socklen_t};
use std::io;
use std::mem;
use std::os::unix::io::RawFd;

pub const MAX_PACKET_SIZE: usize = 1500;
pub const BATCH_SIZE: usize = 64;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Packet {
    pub buf: [u8; MAX_PACKET_SIZE],
    pub len: usize,
    pub addr: sockaddr_storage,
    pub addr_len: socklen_t,

    /// Monotonic timestamp (ns) taken right after recvmmsg returns this packet.
    pub recv_ts_ns: u64,
}

impl Default for Packet {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

pub fn monotonic_now_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };

    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

#[repr(align(64))]
pub struct BatchState {
    pub packets: [Packet; BATCH_SIZE],
    msgs: [mmsghdr; BATCH_SIZE],
    iovecs: [iovec; BATCH_SIZE],
}

unsafe impl Send for BatchState {}

impl BatchState {
    /// Heap-allocates and zero-initializes a `BatchState` directly, without
    /// ever materializing the full struct (tens of KB, grows fast with
    /// bigger batches/buffers) on the stack first the way `Box::new(Self {
    /// ... })` would.
    pub fn new() -> Box<Self> {
        unsafe {
            let layout = std::alloc::Layout::new::<Self>();

            let ptr = std::alloc::alloc_zeroed(layout) as *mut Self;
            if ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }

            Box::from_raw(ptr)
        }
    }

    /// Receives whatever's already queued, up to `BATCH_SIZE` datagrams, in
    /// a single syscall. Never blocks -- returns `Ok(0)` immediately if
    /// nothing is available (MSG_DONTWAIT; recvmmsg's `timeout` parameter
    /// doesn't actually bound the wait for the *first* datagram on this
    /// kernel, so a blocking design isn't viable here.
    pub fn recv_batch(&mut self, fd: RawFd) -> io::Result<usize> {
        for i in 0..BATCH_SIZE {
            self.iovecs[i] = iovec {
                iov_len: MAX_PACKET_SIZE,
                iov_base: self.packets[i].buf.as_mut_ptr() as *mut _,
            };
            self.msgs[i] = mmsghdr {
                msg_hdr: msghdr {
                    msg_name: &mut self.packets[i].addr as *mut _ as *mut _,
                    msg_namelen: mem::size_of::<sockaddr_storage>() as u32,
                    msg_iov: &mut self.iovecs[i] as *mut _,
                    msg_iovlen: 1,
                    msg_control: std::ptr::null_mut(),
                    msg_controllen: 0,
                    msg_flags: 0,
                },
                msg_len: 0,
            }
        }

        let n = unsafe {
            libc::recvmmsg(
                fd,
                self.msgs.as_mut_ptr(),
                BATCH_SIZE as u32,
                libc::MSG_DONTWAIT,
                std::ptr::null_mut(),
            )
        };

        if n < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                return Ok(0);
            }
            return Err(err);
        }
        let n = n as usize;

        let now_ns: u64 = monotonic_now_ns();
        for i in 0..n {
            self.packets[i].len = self.msgs[i].msg_len as usize;
            self.packets[i].addr_len = self.msgs[i].msg_hdr.msg_namelen;
            self.packets[i].recv_ts_ns = now_ns;
        }

        Ok(n)
    }

    /// Sends `packets[0..n]` in a single syscall. Never blocks (MSG_DONTWAIT):
    /// if the socket's send buffer is momentarily full, returns `Ok(0)`
    /// instead of putting the calling (receiver) thread to sleep in the
    /// kernel. That matters here specifically because this same thread also
    /// owns `recv_batch` for this lane -- a blocking send would stall new
    /// receives *and* delay every other reply already waiting in the
    /// outbound ring, not just this batch. Consistent with the rest of the
    /// design: prefer dropping over blocking anywhere in the hot path (same
    /// reasoning as the ring buffers returning `Err`/`None` instead of
    /// blocking).
    ///
    /// A partial send (`0 < returned < n`) is possible too -- sendmmsg
    /// sends what it can and stops at the first message that would block.
    /// The caller (receiver.rs) is responsible for treating
    /// `n - returned` as dropped.
    pub fn send_batch(&mut self, fd: RawFd, n: usize) -> io::Result<usize> {
        debug_assert!(n <= BATCH_SIZE);
        for i in 0..n {
            self.iovecs[i] = iovec {
                iov_base: self.packets[i].buf.as_mut_ptr() as *mut _,
                iov_len: self.packets[i].len,
            };
            self.msgs[i] = mmsghdr {
                msg_hdr: msghdr {
                    msg_name: &mut self.packets[i].addr as *mut _ as *mut _,
                    msg_namelen: self.packets[i].addr_len,
                    msg_iov: &mut self.iovecs[i] as *mut _,
                    msg_iovlen: 1,
                    msg_control: std::ptr::null_mut(),
                    msg_controllen: 0,
                    msg_flags: 0,
                },
                msg_len: 0,
            };
        }

        let sent =
            unsafe { libc::sendmmsg(fd, self.msgs.as_mut_ptr(), n as u32, libc::MSG_DONTWAIT) };

        if sent < 0 {
            let err = io::Error::last_os_error();
            // Send buffer full and not even the first message in this
            // batch could go out right now. Same treatment as ring-full:
            // drop this batch of replies rather than block the thread.
            if err.kind() == io::ErrorKind::WouldBlock {
                return Ok(0);
            }

            return Err(err);
        }
        Ok(sent as usize)
    }
}
