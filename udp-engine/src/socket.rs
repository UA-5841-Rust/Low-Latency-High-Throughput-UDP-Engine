use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::SocketAddr;

/// Creates and binds a UDP socket with `SO_REUSEPORT` set.
///
/// `SO_REUSEPORT` must be set *before* `bind()` -- that's what lets many
/// independent sockets share the same address/port. The kernel then hashes
/// each incoming packet's 4-tuple and delivers it to exactly one socket in
/// the group, so N threads each owning one of these sockets never contend
/// on a shared receive queue the way N threads sharing one `Arc<UdpSocket>`
/// would.
pub fn create_reuseport_socket(addr: SocketAddr) -> io::Result<Socket> {
    let socket = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;

    socket.set_reuse_port(true)?;
    socket.set_reuse_address(true)?;

    // sysctl in setup_env.sh only raises the *ceiling* (net.core.rmem_max /
    // wmem_max); the socket still defaults to net.core.rmem_default unless
    // asked for more explicitly. Setting it here means the binary behaves
    // correctly even if setup_env.sh was never run (just capped lower).
    let _ = socket.set_recv_buffer_size(8 * 1024 * 1024);
    let _ = socket.set_send_buffer_size(8 * 1024 * 1024);

    socket.bind(&addr.into())?;

    // recv_batch (batch.rs) always passes MSG_DONTWAIT itself, so the
    // socket's own blocking/non-blocking mode doesn't affect receiving
    // either way. Left in blocking mode here because send_batch doesn't
    // pass MSG_DONTWAIT -- if the send buffer is ever momentarily full,
    // sendmmsg blocking briefly is an acceptable fallback (rare for UDP;
    // the send buffer is not usually the bottleneck).
    socket.set_nonblocking(false)?;

    Ok(socket)
}
