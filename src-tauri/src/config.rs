use bitcoin::Network;
use once_cell::sync::Lazy;

/// Global configuration for the wallet service
pub struct Config {
    /// Bitcoin network (mainnet, testnet, regtest, signet)
    pub bitcoin_network: &'static str,
}

impl Config {
    fn new() -> Self {
        Self {
            bitcoin_network: env!("BITCOIN_NETWORK"),
        }
    }

    /// Get the Bitcoin network as Network enum
    pub fn network(&self) -> Network {
        parse_network_str(self.bitcoin_network).unwrap_or(Network::Regtest)
    }
}

/// Parse a network string into a Network enum
pub fn parse_network_str(network_str: &str) -> Option<Network> {
    match network_str {
        "testnet" => Some(Network::Testnet),
        "regtest" => Some(Network::Regtest),
        "signet" => Some(Network::Signet),
        "mainnet" | "bitcoin" => Some(Network::Bitcoin),
        _ => None,
    }
}

/// Global config instance
pub static CONFIG: Lazy<Config> = Lazy::new(Config::new);
