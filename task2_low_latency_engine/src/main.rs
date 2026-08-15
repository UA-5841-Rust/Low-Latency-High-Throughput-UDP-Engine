mod net; 
mod server;
mod worker;

use server::UdpEngine;

fn main() {
    let port = 8080;
    let engine = UdpEngine::new(port);

    engine.run();
}