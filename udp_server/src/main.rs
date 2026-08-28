use std::net::UdpSocket;
use std::sync::Arc;
use std::thread;

fn main() {
    let socket = Arc::new(UdpSocket::bind("0.0.0.0:8080").expect("Failed to bind port"));
    let mut handles = vec![];

    println!("Starting baseline UDP server on 4 threads...");

    for _ in 0..4 {
        let sock_clone = Arc::clone(&socket);

        let handle = thread::spawn(move || {
            let mut buf = [0; 2048];

            loop {
                if let Ok((_size, _src)) = sock_clone.recv_from(&mut buf) {
                    std::hint::spin_loop();
                }
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}