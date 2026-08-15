use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::{SocketAddr, UdpSocket};

pub fn bind_reuseport_socket(addr: SocketAddr) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

    socket.set_reuse_port(true)?;

    socket.bind(&addr.into())?;

    Ok(socket.into())
}
