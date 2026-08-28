use std::os::unix::io::RawFd;
use std::mem;

pub fn create_reuseport_socket(port: u16) -> RawFd {
    unsafe {
        let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
        if fd < 0 {
            panic!("Socket creation failed");
        }

        let optval: libc::c_int = 1;
        let ret = libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEPORT,
            &optval as *const _ as *const libc::c_void,
            mem::size_of_val(&optval) as libc::socklen_t,
        );

        if ret < 0 {
            panic!("SO_REUSEPORT failed");
        }

        let mut sockaddr: libc::sockaddr_in = mem::zeroed();
        sockaddr.sin_family = libc::AF_INET as libc::sa_family_t;
        sockaddr.sin_port = port.to_be();
        sockaddr.sin_addr.s_addr = libc::INADDR_ANY;

        let bind_ret = libc::bind(
            fd,
            &sockaddr as *const _ as *const libc::sockaddr,
            mem::size_of_val(&sockaddr) as libc::socklen_t,
        );

        if bind_ret < 0 {
            panic!("Bind failed");
        }

        fd
    }
}

pub const BATCH_SIZE: usize = 64;
pub const PACKET_SIZE: usize = 2048;

#[repr(align(64))]
#[derive(Copy, Clone)]
pub struct Packet {
    pub buffer: [u8; PACKET_SIZE],
    pub len: usize,
}

#[repr(align(64))]
pub struct ReceiveContext {
    pub packets: [Packet; BATCH_SIZE],
    pub iovecs: [libc::iovec; BATCH_SIZE],
    pub msgs: [libc::mmsghdr; BATCH_SIZE],
}

impl ReceiveContext {
    pub fn new() -> Self {
        unsafe { mem::zeroed() }
    }

    pub fn prepare(&mut self) {
        for i in 0..BATCH_SIZE {
            self.iovecs[i].iov_base = self.packets[i].buffer.as_mut_ptr() as *mut libc::c_void;
            self.iovecs[i].iov_len = PACKET_SIZE;
            self.msgs[i].msg_hdr.msg_iov = &mut self.iovecs[i] as *mut libc::iovec;
            self.msgs[i].msg_hdr.msg_iovlen = 1;
        }
    }
}

pub fn receive_batch(fd: RawFd, ctx: &mut ReceiveContext) -> usize {
    let res = unsafe {
        libc::recvmmsg(
            fd,
            ctx.msgs.as_mut_ptr(),
            BATCH_SIZE as libc::c_uint,
            0,
            std::ptr::null_mut(),
        )
    };

    if res > 0 {
        let count = res as usize;
        for i in 0..count {
            ctx.packets[i].len = ctx.msgs[i].msg_len as usize;
        }
        count
    } else {
        0
    }
}
