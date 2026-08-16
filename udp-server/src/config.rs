use std::{env, net::SocketAddr};

pub struct Config {
    address: String,
}

impl Config {
    const SERVER_ADDRESS_ENV_KEY: &str = "SERVER_ADDRESS";
    const SERVER_ADDRESS_DEFAULT: &str = "127.0.0.1:8080";

    pub fn new() -> Self {
        let address = env::var(Self::SERVER_ADDRESS_ENV_KEY)
            .unwrap_or(Self::SERVER_ADDRESS_DEFAULT.to_string());

        let address: SocketAddr = address.parse().expect("invalid server address");

        Self {
            address: address.to_string(),
        }
    }

    pub fn address(&self) -> &str {
        &self.address
    }
}
