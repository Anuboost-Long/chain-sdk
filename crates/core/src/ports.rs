//! Ports capability — see /agent-docs/capabilities/ports/CONTRACT.md.
//! "Could a dev server bind this TCP port right now?", answered by
//! actually binding it — on every address a local dev server commonly
//! uses, because with SO_REUSEADDR (which std sets on Unix, like Node and
//! Kestrel) a bind only conflicts with a server on the exact same address.

use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener};

/// The IPv4 and IPv6 wildcards and loopbacks. Vite on macOS listens on
/// `::1` only, which a `0.0.0.0` probe can't see.
const PROBED: [IpAddr; 4] = [
    IpAddr::V4(Ipv4Addr::UNSPECIFIED),
    IpAddr::V4(Ipv4Addr::LOCALHOST),
    IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    IpAddr::V6(Ipv6Addr::LOCALHOST),
];

/// `true` when a TCP listener could bind `port` on every probed address.
/// Each probe socket is closed straight away. An address this machine
/// doesn't have (no IPv6) is skipped, not counted as busy; a permission
/// refusal counts as busy.
pub fn is_free(port: u16) -> Result<bool, String> {
    if port == 0 {
        return Err("port must be between 1 and 65535".to_string());
    }
    for address in PROBED {
        match TcpListener::bind((address, port)) {
            Ok(_probe) => {}
            Err(e) if matches!(e.kind(), ErrorKind::AddrInUse | ErrorKind::PermissionDenied) => return Ok(false),
            Err(e) if matches!(e.kind(), ErrorKind::AddrNotAvailable | ErrorKind::Unsupported) => {}
            // No ErrorKind of its own: also "no such address here".
            Err(e) if e.raw_os_error() == Some(EAFNOSUPPORT) => {}
            Err(e) => return Err(format!("couldn't probe port {port} on {address}: {e}")),
        }
    }
    Ok(true)
}

#[cfg(target_os = "linux")]
const EAFNOSUPPORT: i32 = 97;
#[cfg(windows)]
const EAFNOSUPPORT: i32 = 10047; // WSAEAFNOSUPPORT
#[cfg(not(any(target_os = "linux", windows)))]
const EAFNOSUPPORT: i32 = 47;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // These probe the machine's real ports; run in parallel, one test's
    // momentary bind or just-released port shows up in another's.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    /// A port nothing else in this test binary uses, found by the OS.
    fn unused_port() -> u16 {
        loop {
            let port = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
            if is_free(port).unwrap() {
                return port;
            }
        }
    }

    #[test]
    fn a_server_on_any_common_address_makes_the_port_busy() {
        let _serial = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        for held in ["0.0.0.0", "127.0.0.1", "::", "::1"] {
            let port = unused_port();
            let server = TcpListener::bind((held, port)).unwrap();
            assert!(!is_free(port).unwrap(), "a server on {held}:{port} read as free");
            drop(server);
            assert!(is_free(port).unwrap(), "{held}:{port} still busy after the server closed");
        }
    }

    #[test]
    fn probing_leaves_nothing_listening() {
        let _serial = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let port = unused_port();
        assert!(is_free(port).unwrap());
        assert!(is_free(port).unwrap());
        TcpListener::bind(("0.0.0.0", port)).unwrap();
    }

    // Below the OS's ephemeral range (49152+ on macOS), so these momentary
    // binds can't collide with the ports other tests get from bind(0).
    #[test]
    fn a_hundred_probes_are_quick() {
        let _serial = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        let started = std::time::Instant::now();
        for port in 20_000..20_100 {
            is_free(port).unwrap();
        }
        assert!(started.elapsed() < std::time::Duration::from_millis(500), "{:?}", started.elapsed());
    }

    #[test]
    fn port_zero_is_rejected() {
        assert!(is_free(0).is_err());
    }
}
