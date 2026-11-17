//! Tráfico de red por app, aproximado y sin privilegios.
//!
//! El kernel no lleva contadores de red por proceso. Lo que sí se puede hacer
//! sin root es:
//!
//! 1. averiguar qué sockets tiene abiertos cada proceso (`/proc/<pid>/fd`
//!    apunta a `socket:[inodo]`), y
//! 2. pedirle al kernel por netlink `sock_diag` la lista de sockets TCP con
//!    su `tcp_info`, que incluye bytes enviados (confirmados) y recibidos.
//!
//! Cruzando ambos por inodo se obtiene el tráfico TCP de la app. Es una
//! aproximación: no cuenta UDP ni otros protocolos, y pierde lo transferido
//! por conexiones que se abren y cierran entre dos muestras.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use netlink_packet_core::{
    NetlinkHeader, NetlinkMessage, NetlinkPayload, NLM_F_DUMP, NLM_F_REQUEST,
};
use netlink_packet_sock_diag::constants::{AF_INET, AF_INET6, IPPROTO_TCP};
use netlink_packet_sock_diag::inet::nlas::Nla;
use netlink_packet_sock_diag::inet::{ExtensionFlags, InetRequest, SocketId, StateFlags};
use netlink_packet_sock_diag::SockDiagMessage;
use netlink_sys::protocols::NETLINK_SOCK_DIAG;
use netlink_sys::{Socket, SocketAddr};

/// Contadores acumulados de un socket TCP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TcpSocket {
    pub inode: u64,
    /// Identificador único del socket en el kernel (distingue inodos reciclados).
    pub cookie: u64,
    /// Bytes enviados y confirmados por el otro extremo.
    pub sent: u64,
    pub received: u64,
}

/// Origen de la lista de sockets TCP del sistema.
pub trait SocketSource {
    /// `None` si la consulta no está disponible.
    fn tcp_sockets(&mut self) -> Option<Vec<TcpSocket>>;
}

/// Consulta real al kernel por netlink `sock_diag`.
#[derive(Debug, Default)]
pub struct NetlinkSource;

impl SocketSource for NetlinkSource {
    fn tcp_sockets(&mut self) -> Option<Vec<TcpSocket>> {
        let mut out = dump(AF_INET)?;
        out.extend(dump(AF_INET6)?);
        Some(out)
    }
}

/// Vuelca todos los sockets TCP de una familia de direcciones.
fn dump(family: u8) -> Option<Vec<TcpSocket>> {
    let mut socket = Socket::new(NETLINK_SOCK_DIAG).ok()?;
    socket.bind_auto().ok()?;
    socket.connect(&SocketAddr::new(0, 0)).ok()?;

    let mut header = NetlinkHeader::default();
    header.flags = NLM_F_REQUEST | NLM_F_DUMP;
    let socket_id = if family == AF_INET6 {
        SocketId::new_v6()
    } else {
        SocketId::new_v4()
    };
    let mut packet = NetlinkMessage::new(
        header,
        SockDiagMessage::InetRequest(InetRequest {
            family,
            protocol: IPPROTO_TCP,
            extensions: ExtensionFlags::INFO,
            states: StateFlags::all(),
            socket_id,
        })
        .into(),
    );
    packet.finalize();
    let mut request = vec![0; packet.buffer_len()];
    packet.serialize(&mut request);
    socket.send(&request, 0).ok()?;

    let mut out = Vec::new();
    let mut buffer = vec![0u8; 64 * 1024];
    // Tope de lecturas: una respuesta mal formada no debe colgar el refresco.
    for _ in 0..4096 {
        let size = socket.recv(&mut &mut buffer[..], 0).ok()?;
        let mut offset = 0;
        while offset < size {
            let message =
                NetlinkMessage::<SockDiagMessage>::deserialize(&buffer[offset..size]).ok()?;
            let length = message.header.length as usize;
            match message.payload {
                NetlinkPayload::Done(_) => return Some(out),
                NetlinkPayload::Error(_) => return None,
                NetlinkPayload::InnerMessage(SockDiagMessage::InetResponse(response)) => {
                    let info = response.nlas.iter().find_map(|nla| match nla {
                        Nla::TcpInfo(info) => Some((info.bytes_acked, info.bytes_received)),
                        _ => None,
                    });
                    let (sent, received) = info.unwrap_or((0, 0));
                    out.push(TcpSocket {
                        inode: response.header.inode as u64,
                        cookie: u64::from_ne_bytes(response.header.socket_id.cookie),
                        sent,
                        received,
                    });
                }
                _ => {}
            }
            if length == 0 {
                break;
            }
            // Los mensajes netlink van alineados a 4 bytes.
            offset += (length + 3) & !3;
        }
    }
    None
}

/// Inodos de los sockets abiertos por un proceso. `None` si su tabla de
/// descriptores no es legible.
pub fn socket_inodes(root: &Path, pid: i32) -> Option<HashSet<u64>> {
    let entries = fs::read_dir(root.join(pid.to_string()).join("fd")).ok()?;
    let mut inodes = HashSet::new();
    for entry in entries.flatten() {
        if let Ok(target) = fs::read_link(entry.path()) {
            if let Some(inode) = parse_socket_link(&target.to_string_lossy()) {
                inodes.insert(inode);
            }
        }
    }
    Some(inodes)
}

/// `socket:[12345]` → `12345`.
pub fn parse_socket_link(target: &str) -> Option<u64> {
    target
        .strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// Tráfico TCP de una app.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NetRate {
    pub down_bps: f64,
    pub up_bps: f64,
    /// Conexiones TCP de la app en este momento.
    pub sockets: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum NetReading {
    /// Sin permiso para ver los descriptores, o netlink no disponible.
    #[default]
    Unavailable,
    /// Primera lectura: todavía no hay con qué comparar.
    Warming,
    Rate(NetRate),
}

pub struct NetTracker {
    root: PathBuf,
    /// (inodo, cookie) → (enviados, recibidos) de la lectura anterior.
    prev: HashMap<(u64, u64), (u64, u64)>,
    last: Option<Instant>,
}

impl NetTracker {
    pub fn new() -> Self {
        Self::with_root("/proc")
    }

    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            prev: HashMap::new(),
            last: None,
        }
    }

    pub fn sample(
        &mut self,
        pids: &[i32],
        now: Instant,
        source: &mut dyn SocketSource,
    ) -> NetReading {
        let mut inodes: HashSet<u64> = HashSet::new();
        let mut readable = false;
        for &pid in pids {
            if let Some(found) = socket_inodes(&self.root, pid) {
                readable = true;
                inodes.extend(found);
            }
        }
        if !readable {
            self.prev.clear();
            self.last = None;
            return NetReading::Unavailable;
        }
        let Some(sockets) = source.tcp_sockets() else {
            self.prev.clear();
            self.last = None;
            return NetReading::Unavailable;
        };

        let elapsed = self
            .last
            .map(|t| now.saturating_duration_since(t).as_secs_f64());
        self.last = Some(now);
        let mut current = HashMap::new();
        let (mut up, mut down) = (0u64, 0u64);
        for socket in sockets.iter().filter(|s| inodes.contains(&s.inode)) {
            let key = (socket.inode, socket.cookie);
            // Una conexión que no estaba en la lectura anterior nació en este
            // intervalo: todo lo que lleva transferido cuenta.
            let (old_sent, old_received) = self.prev.get(&key).copied().unwrap_or((0, 0));
            up += socket.sent.saturating_sub(old_sent);
            down += socket.received.saturating_sub(old_received);
            current.insert(key, (socket.sent, socket.received));
        }
        let count = current.len();
        self.prev = current;

        match elapsed {
            Some(dt) if dt > 0.0 => NetReading::Rate(NetRate {
                down_bps: down as f64 / dt,
                up_bps: up as f64 / dt,
                sockets: count,
            }),
            _ => NetReading::Warming,
        }
    }
}

impl Default for NetTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::os::unix::fs::symlink;
    use std::time::Duration;

    struct Fake(Option<Vec<TcpSocket>>);

    impl SocketSource for Fake {
        fn tcp_sockets(&mut self) -> Option<Vec<TcpSocket>> {
            self.0.clone()
        }
    }

    fn sock(inode: u64, sent: u64, received: u64) -> TcpSocket {
        TcpSocket {
            inode,
            cookie: inode * 10,
            sent,
            received,
        }
    }

    fn add_fd(root: &Path, pid: i32, fd: u32, target: &str) {
        let dir = root.join(pid.to_string()).join("fd");
        fs::create_dir_all(&dir).unwrap();
        symlink(target, dir.join(fd.to_string())).unwrap();
    }

    #[test]
    fn parses_socket_links() {
        assert_eq!(parse_socket_link("socket:[48213]"), Some(48213));
        assert_eq!(parse_socket_link("pipe:[5]"), None);
        assert_eq!(parse_socket_link("/dev/null"), None);
        assert_eq!(parse_socket_link("socket:[x]"), None);
    }

    #[test]
    fn rates_only_count_the_apps_own_sockets() {
        let tmp = tempfile::tempdir().unwrap();
        add_fd(tmp.path(), 1, 3, "socket:[100]");
        add_fd(tmp.path(), 1, 4, "/dev/null");
        add_fd(tmp.path(), 2, 3, "socket:[200]");
        add_fd(tmp.path(), 2, 5, "socket:[100]"); // compartido con el pid 1
        let mut tracker = NetTracker::with_root(tmp.path());
        let t0 = Instant::now();
        let mut first = Fake(Some(vec![
            sock(100, 1000, 5000),
            sock(200, 0, 0),
            sock(999, 7, 7),
        ]));
        assert_eq!(tracker.sample(&[1, 2], t0, &mut first), NetReading::Warming);

        let mut second = Fake(Some(vec![
            sock(100, 3000, 25_000),
            sock(200, 400, 1000),
            sock(999, 9_000_000, 9_000_000),
        ]));
        let reading = tracker.sample(&[1, 2], t0 + Duration::from_secs(2), &mut second);
        assert_eq!(
            reading,
            NetReading::Rate(NetRate {
                down_bps: 10_500.0,
                up_bps: 1200.0,
                sockets: 2
            })
        );
    }

    #[test]
    fn connections_born_between_samples_count_in_full() {
        let tmp = tempfile::tempdir().unwrap();
        add_fd(tmp.path(), 1, 3, "socket:[100]");
        let mut tracker = NetTracker::with_root(tmp.path());
        let t0 = Instant::now();
        tracker.sample(&[1], t0, &mut Fake(Some(vec![sock(100, 10, 10)])));
        add_fd(tmp.path(), 1, 4, "socket:[300]");
        let mut later = Fake(Some(vec![sock(100, 10, 10), sock(300, 500, 2000)]));
        let NetReading::Rate(rate) = tracker.sample(&[1], t0 + Duration::from_secs(1), &mut later)
        else {
            panic!("debe haber tasa");
        };
        assert_eq!(
            (rate.up_bps, rate.down_bps, rate.sockets),
            (500.0, 2000.0, 2)
        );
    }

    #[test]
    fn unreadable_processes_or_no_netlink_are_unavailable() {
        let tmp = tempfile::tempdir().unwrap();
        let mut tracker = NetTracker::with_root(tmp.path());
        let now = Instant::now();
        assert_eq!(
            tracker.sample(&[5], now, &mut Fake(Some(vec![]))),
            NetReading::Unavailable
        );
        add_fd(tmp.path(), 5, 3, "socket:[1]");
        assert_eq!(
            tracker.sample(&[5], now, &mut Fake(None)),
            NetReading::Unavailable
        );
        // Sin sockets TCP no es un error: tasa cero.
        assert_eq!(
            tracker.sample(&[5], now, &mut Fake(Some(vec![]))),
            NetReading::Warming
        );
        let later = now + Duration::from_secs(1);
        assert_eq!(
            tracker.sample(&[5], later, &mut Fake(Some(vec![]))),
            NetReading::Rate(NetRate::default())
        );
    }

    /// Prueba de punta a punta con el kernel real: una conexión TCP local
    /// del propio test debe aparecer con sus bytes.
    #[test]
    fn netlink_reports_the_bytes_of_our_own_loopback_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let me = std::process::id() as i32;
        let mut tracker = NetTracker::new();
        let mut source = NetlinkSource;
        let t0 = Instant::now();
        if tracker.sample(&[me], t0, &mut source) == NetReading::Unavailable {
            // Entorno sin netlink sock_diag (sandbox): no hay nada que probar.
            eprintln!("sock_diag no disponible; se omite");
            return;
        }

        let payload = vec![7u8; 200_000];
        client.write_all(&payload).unwrap();
        let mut received = vec![0u8; payload.len()];
        server.read_exact(&mut received).unwrap();
        // Da tiempo a que el ACK actualice bytes_acked.
        std::thread::sleep(Duration::from_millis(100));

        let NetReading::Rate(rate) =
            tracker.sample(&[me], t0 + Duration::from_secs(1), &mut source)
        else {
            panic!("la segunda lectura debe dar una tasa");
        };
        // El proceso tiene los dos extremos: lo enviado por uno es lo
        // recibido por el otro.
        assert!(rate.up_bps >= 200_000.0, "{rate:?}");
        assert!(rate.down_bps >= 200_000.0, "{rate:?}");
        assert!(rate.sockets >= 2, "{rate:?}");
    }
}
