import Darwin

/// Checks whether something on this machine accepts TCP connections on a port.
enum PortProbe {
    /// Tries the IPv4 and IPv6 loopback addresses: dev servers often listen
    /// on only one of them (e.g. Vite on `::1`).
    @concurrent
    static func isAccepting(_ port: Int) async -> Bool {
        connects(family: AF_INET, port: port) || connects(family: AF_INET6, port: port)
    }

    private static func connects(family: Int32, port: Int) -> Bool {
        let socket = Darwin.socket(family, SOCK_STREAM, IPPROTO_TCP)
        guard socket >= 0 else { return false }
        defer { close(socket) }
        // Connecting to loopback is answered at once: accepted or refused.
        if family == AF_INET {
            var address = sockaddr_in()
            address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
            address.sin_family = sa_family_t(AF_INET)
            address.sin_port = in_port_t(UInt16(port).bigEndian)
            address.sin_addr = in_addr(s_addr: INADDR_LOOPBACK.bigEndian)
            return withUnsafePointer(to: &address) {
                $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    connect(socket, $0, socklen_t(MemoryLayout<sockaddr_in>.size)) == 0
                }
            }
        } else {
            var address = sockaddr_in6()
            address.sin6_len = UInt8(MemoryLayout<sockaddr_in6>.size)
            address.sin6_family = sa_family_t(AF_INET6)
            address.sin6_port = in_port_t(UInt16(port).bigEndian)
            address.sin6_addr = in6addr_loopback
            return withUnsafePointer(to: &address) {
                $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    connect(socket, $0, socklen_t(MemoryLayout<sockaddr_in6>.size)) == 0
                }
            }
        }
    }
}
