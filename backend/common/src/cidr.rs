//! Minimal CIDR parser used by the admin route guard.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::error::{AppError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cidr {
    network: IpAddr,
    prefix: u8,
}

impl Cidr {
    pub fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        let (addr, prefix) = match trimmed.split_once('/') {
            Some((addr, prefix)) => {
                let prefix = prefix
                    .parse::<u8>()
                    .map_err(|_| AppError::invalid(format!("invalid CIDR prefix in {raw}")))?;
                (addr, Some(prefix))
            }
            None => (trimmed, None),
        };
        let network = addr
            .parse::<IpAddr>()
            .map_err(|_| AppError::invalid(format!("invalid CIDR address in {raw}")))?;
        let max_prefix = match network {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        let prefix = prefix.unwrap_or(max_prefix);
        if prefix > max_prefix {
            return Err(AppError::invalid(format!(
                "CIDR prefix out of range in {raw}"
            )));
        }
        Ok(Self { network, prefix })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, ip) {
            (IpAddr::V4(network), IpAddr::V4(ip)) => prefix_match_v4(network, ip, self.prefix),
            (IpAddr::V6(network), IpAddr::V6(ip)) => prefix_match_v6(network, ip, self.prefix),
            _ => false,
        }
    }
}

fn prefix_match_v4(network: Ipv4Addr, ip: Ipv4Addr, prefix: u8) -> bool {
    if prefix == 0 {
        return true;
    }
    let mask = u32::MAX << (u32::BITS - u32::from(prefix));
    (u32::from(network) & mask) == (u32::from(ip) & mask)
}

fn prefix_match_v6(network: Ipv6Addr, ip: Ipv6Addr, prefix: u8) -> bool {
    if prefix == 0 {
        return true;
    }
    let mask = u128::MAX << (u128::BITS - u32::from(prefix));
    (u128::from(network) & mask) == (u128::from(ip) & mask)
}

/// Parses a list of CIDR strings, failing fast on the first invalid entry.
pub fn parse_list(entries: &[String]) -> Result<Vec<Cidr>> {
    entries.iter().map(|entry| Cidr::parse(entry)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
    }

    #[test]
    fn inside_and_outside_v4_range() {
        let cidr = Cidr::parse("10.0.0.0/8").expect("valid cidr");
        assert!(cidr.contains(ip("10.1.2.3")));
        assert!(cidr.contains(ip("10.255.255.255")));
        assert!(!cidr.contains(ip("192.168.1.1")));
        assert!(!cidr.contains(ip("9.255.255.255")));
    }

    #[test]
    fn exact_ip_and_all_prefix() {
        let exact = Cidr::parse("127.0.0.1").expect("valid cidr");
        assert!(exact.contains(ip("127.0.0.1")));
        assert!(!exact.contains(ip("127.0.0.2")));
        let all = Cidr::parse("0.0.0.0/0").expect("valid cidr");
        assert!(all.contains(ip("203.0.113.5")));
    }

    #[test]
    fn v6_range_does_not_match_v4() {
        let cidr = Cidr::parse("fd00::/8").expect("valid cidr");
        assert!(cidr.contains(IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1))));
        assert!(!cidr.contains(ip("10.0.0.1")));
    }

    #[test]
    fn invalid_entries_are_rejected() {
        assert!(Cidr::parse("10.0.0.0/33").is_err());
        assert!(Cidr::parse("not-an-ip/8").is_err());
        assert!(Cidr::parse("fd00::/129").is_err());
    }

    #[test]
    fn list_parsing_reports_failure() {
        let parsed = parse_list(&["10.0.0.0/8".to_string(), "192.168.0.0/16".to_string()])
            .expect("valid list");
        assert_eq!(parsed.len(), 2);
        assert!(parse_list(&["bad/8".to_string()]).is_err());
    }
}
