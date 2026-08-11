use actix_web::HttpRequest;
use sha2::{Digest, Sha256};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::OnceLock;

/// Environment variable: comma-separated CIDRs of reverse proxies that may set
/// `X-Forwarded-For` / `X-Real-IP`. When the peer is **not** in this set, those
/// headers are ignored and the transport peer address is used.
pub const TRUSTED_PROXY_CIDRS_ENV: &str = "TRUSTED_PROXY_CIDRS";

#[derive(Debug, Clone, Copy)]
pub enum Cidr {
    V4 { network: u32, mask: u32 },
    V6 { network: u128, mask: u128 },
}

impl Cidr {
    fn contains(self, ip: IpAddr) -> bool {
        match (self, ip) {
            (Cidr::V4 { network, mask }, IpAddr::V4(addr)) => (u32::from(addr) & mask) == (network & mask),
            (Cidr::V6 { network, mask }, IpAddr::V6(addr)) => (u128::from(addr) & mask) == (network & mask),
            _ => false,
        }
    }
}

impl FromStr for Cidr {
    type Err = ();

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(());
        }
        let (addr_part, prefix_len) = match trimmed.split_once('/') {
            Some((addr, len)) => (addr.trim(), len.trim().parse::<u8>().map_err(|_| ())?),
            None => (trimmed, if trimmed.contains(':') { 128 } else { 32 }),
        };
        let ip = parse_ip_addr(addr_part).ok_or(())?;
        match ip {
            IpAddr::V4(addr) => {
                if prefix_len > 32 {
                    return Err(());
                }
                let mask = if prefix_len == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix_len)
                };
                Ok(Cidr::V4 {
                    network: u32::from(addr),
                    mask,
                })
            }
            IpAddr::V6(addr) => {
                if prefix_len > 128 {
                    return Err(());
                }
                let mask = if prefix_len == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix_len)
                };
                Ok(Cidr::V6 {
                    network: u128::from(addr),
                    mask,
                })
            }
        }
    }
}

fn trusted_proxy_cidrs() -> &'static [Cidr] {
    static CIDRS: OnceLock<Vec<Cidr>> = OnceLock::new();
    CIDRS
        .get_or_init(|| {
            std::env::var(TRUSTED_PROXY_CIDRS_ENV)
                .unwrap_or_default()
                .split(',')
                .filter_map(|part| Cidr::from_str(part).ok())
                .collect()
        })
        .as_slice()
}

/// Test/helper: parse a CIDR list without touching process-global env cache.
pub fn parse_trusted_proxy_cidrs(raw: &str) -> Vec<Cidr> {
    raw.split(',').filter_map(|part| Cidr::from_str(part).ok()).collect()
}

fn peer_ip(req: &HttpRequest) -> Option<IpAddr> {
    req.peer_addr().map(|addr| addr.ip())
}

fn is_trusted_ip(ip: IpAddr, cidrs: &[Cidr]) -> bool {
    !cidrs.is_empty() && cidrs.iter().any(|cidr| cidr.contains(ip))
}

/// Parse XFF hops (left → right: original client … nearer proxies).
fn parse_xff_hops(xff: &str) -> Vec<IpAddr> {
    xff.split(',').filter_map(|part| parse_ip_addr(part.trim())).collect()
}

/// From the right (nearest hop), skip addresses in `trusted_cidrs` and return
/// the first untrusted address — the real client. Resists client-injected
/// prefixes on the left of XFF.
///
/// `peer` is the immediate transport hop (already verified as trusted).
pub fn client_ip_from_forwarded_chain(xff_hops: &[IpAddr], peer: IpAddr, trusted_cidrs: &[Cidr]) -> IpAddr {
    // Walk XFF from right to left (closest proxy toward original client).
    for hop in xff_hops.iter().rev() {
        if !is_trusted_ip(*hop, trusted_cidrs) {
            return *hop;
        }
    }
    // Entire XFF chain was trusted (or empty) — fall back to peer.
    peer
}

fn forwarded_client_ip(req: &HttpRequest, peer: IpAddr, trusted_cidrs: &[Cidr]) -> Option<IpAddr> {
    if let Some(value) = req
        .headers()
        .get("X-Forwarded-For")
        .and_then(|value| value.to_str().ok())
    {
        let hops = parse_xff_hops(value);
        if !hops.is_empty() {
            return Some(client_ip_from_forwarded_chain(&hops, peer, trusted_cidrs));
        }
    }

    // Single-hop X-Real-IP only when peer is trusted; treat as the sole claim.
    if let Some(value) = req.headers().get("X-Real-IP").and_then(|value| value.to_str().ok()) {
        if let Some(ip) = parse_ip_addr(value.trim()) {
            if !is_trusted_ip(ip, trusted_cidrs) {
                return Some(ip);
            }
        }
    }

    None
}

/// Resolve the real client IP.
///
/// - If the transport peer is listed in `TRUSTED_PROXY_CIDRS`, walk XFF from
///   the right, stripping trusted proxy hops, and take the first untrusted IP.
/// - Otherwise ignore spoofable forwarding headers and use the peer address.
/// - If there is no transport peer, return `None` (never re-trust via
///   `connection_info` / forwarded headers).
pub fn extract_client_ip(req: &HttpRequest) -> Option<String> {
    extract_client_ip_with_cidrs(req, trusted_proxy_cidrs())
}

pub fn extract_client_ip_with_cidrs(req: &HttpRequest, trusted_cidrs: &[Cidr]) -> Option<String> {
    let Some(peer) = peer_ip(req) else {
        // No peer socket — unit tests or misconfigured acceptors. Do not
        // consult connection_info / XFF (those can reintroduce spoofing).
        return None;
    };

    if is_trusted_ip(peer, trusted_cidrs) {
        if let Some(client) = forwarded_client_ip(req, peer, trusted_cidrs) {
            return Some(client.to_string());
        }
    }

    Some(peer.to_string())
}

pub fn extract_user_agent(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("User-Agent")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub fn build_user_agent_hash(raw_user_agent: Option<&str>) -> String {
    let normalized = raw_user_agent
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| "unknown".to_string());

    sha256_hex(normalized.as_bytes())
}

pub fn build_ip_subnet_hash(raw_ip: Option<&str>) -> String {
    let subnet = raw_ip
        .and_then(parse_ip_addr)
        .map(normalize_ip_subnet)
        .unwrap_or_else(|| "unknown".to_string());

    sha256_hex(subnet.as_bytes())
}

fn parse_ip_addr(raw_ip: &str) -> Option<IpAddr> {
    let trimmed = raw_ip.trim().trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty() {
        return None;
    }

    trimmed
        .parse::<IpAddr>()
        .ok()
        .or_else(|| trimmed.parse::<SocketAddr>().ok().map(|addr| addr.ip()))
}

fn normalize_ip_subnet(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(addr) => {
            let octets = addr.octets();
            format!("{}.{}.{}.0/24", octets[0], octets[1], octets[2])
        }
        IpAddr::V6(addr) => {
            let segments = addr.segments();
            format!(
                "{:x}:{:x}:{:x}:{:x}::/64",
                segments[0], segments[1], segments[2], segments[3]
            )
        }
    }
}

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn cidr_contains_expected_addresses() {
        let cidr = Cidr::from_str("10.0.0.0/8").expect("cidr");
        assert!(cidr.contains(IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))));
        assert!(!cidr.contains(IpAddr::V4(Ipv4Addr::new(11, 0, 0, 1))));
    }

    #[test]
    fn untrusted_peer_ignores_spoofed_forwarded_for() {
        let req = TestRequest::default()
            .peer_addr("198.51.100.10:40000".parse().expect("peer"))
            .insert_header(("X-Forwarded-For", "203.0.113.9"))
            .to_http_request();
        let cidrs = parse_trusted_proxy_cidrs("10.0.0.0/8");
        assert_eq!(
            extract_client_ip_with_cidrs(&req, &cidrs).as_deref(),
            Some("198.51.100.10")
        );
    }

    #[test]
    fn trusted_peer_uses_rightmost_untrusted_client_ip() {
        let req = TestRequest::default()
            .peer_addr("10.0.0.5:40000".parse().expect("peer"))
            .insert_header(("X-Forwarded-For", "203.0.113.50, 10.0.0.5"))
            .to_http_request();
        let cidrs = parse_trusted_proxy_cidrs("10.0.0.0/8");
        assert_eq!(
            extract_client_ip_with_cidrs(&req, &cidrs).as_deref(),
            Some("203.0.113.50")
        );
    }

    #[test]
    fn multi_proxy_chain_strips_trusted_hops_from_right() {
        // client, edge-proxy, peer — both 10/8 are trusted proxies.
        let hops = vec!["203.0.113.77".parse().unwrap(), "10.0.0.8".parse().unwrap()];
        let peer: IpAddr = "10.0.0.5".parse().unwrap();
        let cidrs = parse_trusted_proxy_cidrs("10.0.0.0/8");
        assert_eq!(
            client_ip_from_forwarded_chain(&hops, peer, &cidrs).to_string(),
            "203.0.113.77"
        );
    }

    #[test]
    fn malicious_xff_prefix_is_ignored() {
        // Attacker injects 1.2.3.4 on the left; real client is 203.0.113.50.
        let req = TestRequest::default()
            .peer_addr("10.0.0.5:40000".parse().expect("peer"))
            .insert_header(("X-Forwarded-For", "1.2.3.4, 203.0.113.50, 10.0.0.9"))
            .to_http_request();
        let cidrs = parse_trusted_proxy_cidrs("10.0.0.0/8");
        assert_eq!(
            extract_client_ip_with_cidrs(&req, &cidrs).as_deref(),
            Some("203.0.113.50")
        );
    }

    #[test]
    fn empty_trusted_list_never_honours_forwarded_headers() {
        let req = TestRequest::default()
            .peer_addr("10.0.0.5:40000".parse().expect("peer"))
            .insert_header(("X-Forwarded-For", "203.0.113.50"))
            .to_http_request();
        assert_eq!(extract_client_ip_with_cidrs(&req, &[]).as_deref(), Some("10.0.0.5"));
    }

    #[test]
    fn no_peer_never_trusts_forwarded_or_connection_info() {
        let req = TestRequest::default()
            .insert_header(("X-Forwarded-For", "203.0.113.50"))
            .insert_header(("X-Real-IP", "198.51.100.1"))
            .to_http_request();
        let cidrs = parse_trusted_proxy_cidrs("10.0.0.0/8,0.0.0.0/0");
        assert_eq!(extract_client_ip_with_cidrs(&req, &cidrs), None);
    }

    #[test]
    fn ipv6_trusted_proxy_chain() {
        let hops = vec!["2001:db8::1".parse().unwrap(), "fd00::2".parse().unwrap()];
        let peer: IpAddr = "fd00::1".parse().unwrap();
        let cidrs = parse_trusted_proxy_cidrs("fd00::/8");
        assert_eq!(
            client_ip_from_forwarded_chain(&hops, peer, &cidrs),
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1))
        );
    }
}
