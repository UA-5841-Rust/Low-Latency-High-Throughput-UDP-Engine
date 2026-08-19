mod config;

use config::Config;
use std::{io, net::UdpSocket, sync::Arc, thread};

const BUFFER_SIZE: usize = 1500;

fn main() -> io::Result<()> {
    let config = Config::new();

    let socket = UdpSocket::bind(config.address())?;
    let socket = Arc::new(socket);
    println!("udp server is listening on {}", config.address());

    let num_workers = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);

    let mut handles = vec![];

    for _ in 0..num_workers {
        let socket = Arc::clone(&socket);

        let handle = thread::spawn(move || {
            let mut buf: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];

            loop {
                let (num_of_bytes, src_addr) = match socket.recv_from(&mut buf) {
                    Ok((n, a)) => (n, a),
                    Err(err) => {
                        eprintln!("receive datagram message on the socket: {:#?}", err);
                        continue;
                    }
                };

                let msg_bytes = &buf[..num_of_bytes];

                // println!(
                //     "(worker #{}) message from {}: {:?} (bytes received: {})",
                //     id,
                //     src_addr,
                //     String::from_utf8_lossy(msg_bytes).trim_end(),
                //     num_of_bytes,
                // );

                if let Err(err) = socket.send_to(msg_bytes, src_addr) {
                    eprintln!("send data on the socket: {:#?}", err);
                };
            }
        });

        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    Ok(())
}
