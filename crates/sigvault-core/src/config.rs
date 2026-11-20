use bitcoin::Network;

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
