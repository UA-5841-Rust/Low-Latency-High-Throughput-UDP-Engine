mod server;
mod worker;

use server::UdpServer;

fn main() {
    let port = 8080;
    let server = UdpServer::new(port);

    server.run();
}