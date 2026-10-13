use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

/// Whether something on this machine accepts TCP connections on `port`.
/// Tries the IPv4 and IPv6 loopback addresses: dev servers often listen on
/// only one of them (e.g. Vite on `::1`).
pub(crate) async fn is_accepting(port: u16) -> bool {
    connects(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await
        || connects(SocketAddr::from((Ipv6Addr::LOCALHOST, port))).await
}

async fn connects(address: SocketAddr) -> bool {
    // Connecting to loopback is answered at once, accepted or refused; the
    // timeout only guards against a firewall that drops instead.
    matches!(tokio::time::timeout(Duration::from_secs(1), tokio::net::TcpStream::connect(address)).await, Ok(Ok(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sees_a_listener_on_either_loopback_address() {
        for address in ["127.0.0.1:0", "[::1]:0"] {
            let Ok(listener) = std::net::TcpListener::bind(address) else { continue };
            let port = listener.local_addr().unwrap().port();
            assert!(is_accepting(port).await, "{address}");
            drop(listener);
            assert!(!is_accepting(port).await, "{address} closed");
        }
    }
}
