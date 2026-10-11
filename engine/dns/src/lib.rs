//! AdNox DNS filtering server.
//!
//! A tiny asynchronous UDP DNS proxy. For every incoming query:
//!   * if the queried hostname is on the blocklist, it answers with a *sinkhole* response
//!     (`0.0.0.0` for A, `::` for AAAA, empty NOERROR for anything else) so the client
//!     never connects to the ad/tracker host;
//!   * otherwise it forwards the query verbatim to an upstream resolver and relays the reply.
//!
//! This is the shared blocking mechanism for both platforms:
//!   * **Windows** – run as a background service on 127.0.0.1:53 and set the system DNS to it.
//!   * **Android** – the `VpnService` hands captured DNS packets to this logic via the FFI layer.
//!
//! Note: Brave's `adblock::Engine` is `!Send` (it uses thread-local pools internally), so the
//! [`Blocklist`] lives on the accept loop and is never moved into a worker task. Only the
//! upstream *forward* — which needs no engine — is spawned concurrently.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use adnox_core::Blocklist;
use hickory_proto::op::{Message, MessageType, OpCode, ResponseCode};
use hickory_proto::rr::rdata::{A, AAAA};
use hickory_proto::rr::{DNSClass, RData, Record, RecordType};
use hickory_proto::serialize::binary::BinDecodable;
use tokio::net::UdpSocket;

/// Runtime statistics shared across tasks.
#[derive(Default)]
pub struct Stats {
    pub queries: AtomicU64,
    pub blocked: AtomicU64,
    pub forwarded: AtomicU64,
    pub errors: AtomicU64,
}

impl Stats {
    pub fn snapshot(&self) -> StatsSnapshot {
        StatsSnapshot {
            queries: self.queries.load(Ordering::Relaxed),
            blocked: self.blocked.load(Ordering::Relaxed),
            forwarded: self.forwarded.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StatsSnapshot {
    pub queries: u64,
    pub blocked: u64,
    pub forwarded: u64,
    pub errors: u64,
}

pub struct ServerConfig {
    /// Address to listen on, e.g. `127.0.0.1:53` or `0.0.0.0:5353`.
    pub bind: SocketAddr,
    /// Upstream resolver to forward allowed queries to, e.g. `1.1.1.1:53`.
    pub upstream: SocketAddr,
    /// TTL (seconds) used on sinkhole answers.
    pub sinkhole_ttl: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            bind: "127.0.0.1:53".parse().unwrap(),
            upstream: "1.1.1.1:53".parse().unwrap(),
            sinkhole_ttl: 60,
        }
    }
}

/// The outcome of inspecting a single query, surfaced to callers for logging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Blocked(String),
    Forwarded(String),
    Malformed,
}

/// What the server should physically do with a query, produced by [`decide`].
pub enum Action {
    /// Reply immediately with these response bytes (a sinkhole answer).
    Reply(Vec<u8>),
    /// Forward the query upstream and relay the reply.
    Forward,
    /// The query could not be parsed; drop it.
    Drop,
}

/// Pure, synchronous decision step. Parses the query, consults the blocklist, and — when the
/// domain is blocked — builds the sinkhole response. Performs no I/O and touches no stats, so it
/// can run on the accept loop (where the `!Send` [`Blocklist`] lives) without blocking workers.
pub fn decide(blocklist: &Blocklist, cfg: &ServerConfig, query: &[u8]) -> (Action, Decision) {
    let request = match Message::from_bytes(query) {
        Ok(m) => m,
        Err(_) => return (Action::Drop, Decision::Malformed),
    };
    let question = match request.queries().first() {
        Some(q) => q,
        None => return (Action::Drop, Decision::Malformed),
    };
    let host = question.name().to_ascii().trim_end_matches('.').to_string();

    if blocklist.is_blocked_domain(&host) {
        let resp = build_sinkhole(&request, cfg.sinkhole_ttl);
        (Action::Reply(resp), Decision::Blocked(host))
    } else {
        (Action::Forward, Decision::Forwarded(host))
    }
}

/// Transport-agnostic async handler used by the Android VPN loop and by tests. Given raw query
/// bytes it returns the response bytes, calling `forward` to fetch the upstream reply for allowed
/// queries. (The UDP server below does not use this; it splits the steps so forwarding can be
/// spawned without moving the engine.)
pub async fn handle_query<F, Fut>(
    blocklist: &Blocklist,
    cfg: &ServerConfig,
    stats: &Stats,
    query: &[u8],
    forward: F,
) -> (Vec<u8>, Decision)
where
    F: FnOnce(Vec<u8>) -> Fut,
    Fut: std::future::Future<Output = std::io::Result<Vec<u8>>>,
{
    stats.queries.fetch_add(1, Ordering::Relaxed);
    let (action, decision) = decide(blocklist, cfg, query);
    match action {
        Action::Reply(resp) => {
            stats.blocked.fetch_add(1, Ordering::Relaxed);
            (resp, decision)
        }
        Action::Drop => {
            stats.errors.fetch_add(1, Ordering::Relaxed);
            (Vec::new(), decision)
        }
        Action::Forward => match forward(query.to_vec()).await {
            Ok(bytes) => {
                stats.forwarded.fetch_add(1, Ordering::Relaxed);
                (bytes, decision)
            }
            Err(_) => {
                stats.errors.fetch_add(1, Ordering::Relaxed);
                let resp = Message::from_bytes(query)
                    .map(|m| build_servfail(&m))
                    .unwrap_or_default();
                (resp, decision)
            }
        },
    }
}

/// Build a sinkhole response: `0.0.0.0` for A, `::` for AAAA, empty NOERROR otherwise.
fn build_sinkhole(request: &Message, ttl: u32) -> Vec<u8> {
    let mut resp = base_response(request);
    resp.set_response_code(ResponseCode::NoError);

    if let Some(q) = request.queries().first() {
        match q.query_type() {
            RecordType::A => {
                let mut rec = Record::with(q.name().clone(), RecordType::A, ttl);
                rec.set_dns_class(DNSClass::IN);
                rec.set_data(Some(RData::A(A(Ipv4Addr::UNSPECIFIED))));
                resp.add_answer(rec);
            }
            RecordType::AAAA => {
                let mut rec = Record::with(q.name().clone(), RecordType::AAAA, ttl);
                rec.set_dns_class(DNSClass::IN);
                rec.set_data(Some(RData::AAAA(AAAA(Ipv6Addr::UNSPECIFIED))));
                resp.add_answer(rec);
            }
            // For other record types return NOERROR with no answers (NODATA).
            _ => {}
        }
    }

    resp.to_vec().unwrap_or_default()
}

fn build_servfail(request: &Message) -> Vec<u8> {
    let mut resp = base_response(request);
    resp.set_response_code(ResponseCode::ServFail);
    resp.to_vec().unwrap_or_default()
}

fn base_response(request: &Message) -> Message {
    let mut resp = Message::new();
    resp.set_id(request.id());
    resp.set_message_type(MessageType::Response);
    resp.set_op_code(OpCode::Query);
    resp.set_recursion_desired(request.recursion_desired());
    resp.set_recursion_available(true);
    for q in request.queries() {
        resp.add_query(q.clone());
    }
    resp
}

/// Run the UDP DNS server until the process is killed. Blocks (async) forever.
///
/// `blocklist` is borrowed, not shared across threads: `adblock::Engine` is `!Send`, so the
/// engine stays on this task (run via `block_on`), and only the upstream forward — which needs
/// no engine — is spawned onto worker threads.
pub async fn run_udp(
    blocklist: &Blocklist,
    cfg: Arc<ServerConfig>,
    stats: Arc<Stats>,
) -> std::io::Result<()> {
    let socket = Arc::new(UdpSocket::bind(cfg.bind).await?);
    let mut buf = vec![0u8; 1500];

    loop {
        let (len, peer) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let query = buf[..len].to_vec();
        stats.queries.fetch_add(1, Ordering::Relaxed);

        // Decide synchronously on the loop thread (the engine never leaves this thread).
        let (action, _decision) = decide(blocklist, &cfg, &query);
        match action {
            Action::Reply(resp) => {
                stats.blocked.fetch_add(1, Ordering::Relaxed);
                let _ = socket.send_to(&resp, peer).await;
            }
            Action::Drop => {
                stats.errors.fetch_add(1, Ordering::Relaxed);
            }
            Action::Forward => {
                // Only the forward needs to be spawned, and it carries no engine.
                let socket = socket.clone();
                let upstream = cfg.upstream;
                let stats = stats.clone();
                tokio::spawn(async move {
                    match forward_udp(query.clone(), upstream).await {
                        Ok(bytes) => {
                            stats.forwarded.fetch_add(1, Ordering::Relaxed);
                            let _ = socket.send_to(&bytes, peer).await;
                        }
                        Err(_) => {
                            stats.errors.fetch_add(1, Ordering::Relaxed);
                            if let Ok(m) = Message::from_bytes(&query) {
                                let _ = socket.send_to(&build_servfail(&m), peer).await;
                            }
                        }
                    }
                });
            }
        }
    }
}

/// Forward a raw DNS query to an upstream UDP resolver and return the raw response.
async fn forward_udp(query: Vec<u8>, upstream: SocketAddr) -> std::io::Result<Vec<u8>> {
    let bind: SocketAddr = if upstream.is_ipv4() {
        "0.0.0.0:0".parse().unwrap()
    } else {
        "[::]:0".parse().unwrap()
    };
    let sock = UdpSocket::bind(bind).await?;
    sock.connect(upstream).await?;
    sock.send(&query).await?;

    let mut buf = vec![0u8; 1500];
    let len = tokio::time::timeout(Duration::from_secs(5), sock.recv(&mut buf))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "upstream timeout"))??;
    buf.truncate(len);
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::op::Query;
    use hickory_proto::rr::Name;
    use std::str::FromStr;

    fn make_query(host: &str, rtype: RecordType) -> Vec<u8> {
        let mut msg = Message::new();
        msg.set_id(0x1234);
        msg.set_message_type(MessageType::Query);
        msg.set_op_code(OpCode::Query);
        msg.set_recursion_desired(true);
        let mut q = Query::new();
        q.set_name(Name::from_str(host).unwrap());
        q.set_query_type(rtype);
        q.set_query_class(DNSClass::IN);
        msg.add_query(q);
        msg.to_vec().unwrap()
    }

    #[test]
    fn blocked_domain_builds_sinkhole() {
        let bl = Blocklist::from_lists(&["||doubleclick.net^".to_string()]);
        let cfg = ServerConfig::default();
        let query = make_query("ads.doubleclick.net.", RecordType::A);

        let (action, decision) = decide(&bl, &cfg, &query);
        assert!(matches!(decision, Decision::Blocked(_)));
        let resp = match action {
            Action::Reply(bytes) => bytes,
            _ => panic!("expected a sinkhole reply"),
        };
        let msg = Message::from_bytes(&resp).unwrap();
        assert_eq!(msg.id(), 0x1234);
        assert_eq!(msg.answer_count(), 1);
        match msg.answers()[0].data() {
            Some(RData::A(a)) => assert_eq!(a.0, Ipv4Addr::UNSPECIFIED),
            other => panic!("expected A 0.0.0.0, got {other:?}"),
        }
    }

    #[test]
    fn allowed_domain_is_forwarded() {
        let bl = Blocklist::from_lists(&["||doubleclick.net^".to_string()]);
        let cfg = ServerConfig::default();
        let query = make_query("example.com.", RecordType::A);
        let (action, decision) = decide(&bl, &cfg, &query);
        assert!(matches!(decision, Decision::Forwarded(_)));
        assert!(matches!(action, Action::Forward));
    }

    #[tokio::test]
    async fn handle_query_forwards_via_callback() {
        let bl = Blocklist::from_lists(&["||doubleclick.net^".to_string()]);
        let cfg = ServerConfig::default();
        let stats = Stats::default();
        let query = make_query("example.com.", RecordType::A);
        let canned = make_query("example.com.", RecordType::A);
        let canned_clone = canned.clone();

        let (resp, decision) =
            handle_query(
                &bl,
                &cfg,
                &stats,
                &query,
                |_b| async move { Ok(canned_clone) },
            )
            .await;
        assert!(matches!(decision, Decision::Forwarded(_)));
        assert_eq!(resp, canned);
        assert_eq!(stats.forwarded.load(Ordering::Relaxed), 1);
    }
}
