use std::net::{SocketAddr, UdpSocket};
use std::os::unix::io::AsRawFd;
use std::thread;
use std::time::Duration;

const BATCH_SIZE: usize = 64;
const PACKET_SIZE: usize = 64;

fn main() {
    let target = "127.0.0.1:8080";
    let threads = 2;

    println!("running udp flood on {}, threads: {}...", target, threads);

    for i in 0..threads {
        thread::spawn(move || {
            let src_port = 30000 + i as u16;
            let socket =
                UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], src_port))).expect("bind failed");
            socket.connect(target).expect("connect failed");
            socket.set_nonblocking(true).unwrap();
            let fd = socket.as_raw_fd();

            let payload = [0u8; PACKET_SIZE];

            let mut iovecs = [libc::iovec {
                iov_base: payload.as_ptr() as *mut _,
                iov_len: PACKET_SIZE,
            }; BATCH_SIZE];

            let mut msgs = [libc::mmsghdr {
                msg_hdr: libc::msghdr {
                    msg_name: std::ptr::null_mut(),
                    msg_namelen: 0,
                    msg_iov: std::ptr::null_mut(),
                    msg_iovlen: 1,
                    msg_control: std::ptr::null_mut(),
                    msg_controllen: 0,
                    msg_flags: 0,
                },
                msg_len: 0,
            }; BATCH_SIZE];

            for j in 0..BATCH_SIZE {
                msgs[j].msg_hdr.msg_iov = &mut iovecs[j] as *mut _;
            }

            let mut discard_buf = [0u8; 2048];

            println!("thread {} is ready...", i);

            loop {
                unsafe {
                    libc::sendmmsg(fd, msgs.as_mut_ptr(), BATCH_SIZE as u32, libc::MSG_DONTWAIT);
                }

                while socket.recv(&mut discard_buf).is_ok() {}
            }
        });
    }

    loop {
        thread::sleep(Duration::from_secs(1));
    }
}
