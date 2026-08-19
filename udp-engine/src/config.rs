use std::env;
use std::net::SocketAddr;

pub struct Config {
    address: SocketAddr,
}

impl Config {
    const SERVER_ADDRESS_ENV_KEY: &str = "SERVER_ADDRESS";
    const SERVER_ADDRESS_DEFAULT: &str = "127.0.0.1:8080";

    pub fn new() -> Self {
        let address = env::var(Self::SERVER_ADDRESS_ENV_KEY)
            .unwrap_or_else(|_| Self::SERVER_ADDRESS_DEFAULT.to_string());

        let address: SocketAddr = address.parse().expect("invalid SERVER_ADDRESS");

        Self { address: address }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}
