use std::net::UdpSocket;

fn main() -> std::io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:8080")?;
    let mut buf: [u8; 512] = [0; 512];

    println!("Server started at 127.0.0.1:8080");

    loop {
        socket.recv_from(&mut buf)?;

        println!("{:#?}", buf);
    }
}
