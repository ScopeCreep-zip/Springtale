//! Integration tests for [`HttpTransport`] — the rustls mutual-TLS transport.
//!
//! Three properties are covered:
//!
//! 1. Two real `HttpTransport` nodes, issued certificates by a CA generated
//!    in-test, round-trip a [`Message`] end to end.
//! 2. A peer that trusts the wrong CA, or presents a certificate from the
//!    wrong CA, is refused at the TLS layer. Each refusal is pinned to the
//!    exact rustls failure (`UnknownIssuer` / a fatal certificate alert) via
//!    a raw rustls probe against the *real* transport server, so the test
//!    cannot pass if certificate verification were disabled.
//! 3. The handshake against the real transport server negotiates the hybrid
//!    post-quantum group `X25519MLKEM768`, asserted from
//!    [`rustls::CommonState::negotiated_key_exchange_group`] on a completed
//!    connection — handshake state, not configuration.
//!
//! The probes drive `rustls::ClientConnection` by hand
//! (`read_tls`/`write_tls`/`process_new_packets`) rather than through
//! `complete_io`, because that is the only path that surfaces the typed
//! [`rustls::Error`] instead of an opaque `io::Error`.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{CertificateError, ClientConfig, ClientConnection, NamedGroup, RootCertStore};
use tempfile::TempDir;
use uuid::Uuid;

use springtale_crypto::identity::NodeId;
use springtale_transport::error::TransportError;
use springtale_transport::http::{HttpTransport, HttpTransportConfig};
use springtale_transport::transport::{Message, Transport};

// ── Test PKI ──────────────────────────────────────────────────────

/// A throwaway certificate authority backed by an in-test key pair.
struct TestCa {
    cert: Certificate,
    key: KeyPair,
}

impl TestCa {
    fn new(common_name: &str) -> Self {
        let key = KeyPair::generate().expect("generate CA key");
        let mut params = CertificateParams::new(Vec::<String>::new()).expect("CA params");
        params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(1));
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let cert = params.self_signed(&key).expect("self-sign CA");
        Self { cert, key }
    }

    fn der(&self) -> CertificateDer<'static> {
        self.cert.der().clone()
    }

    /// Issue an end-entity certificate valid for both TLS roles, so the same
    /// material can serve `HttpTransport`'s listener and its outbound client.
    fn issue_leaf(&self, common_name: &str) -> (String, String) {
        let key = KeyPair::generate().expect("generate leaf key");
        let mut params =
            CertificateParams::new(vec!["localhost".to_string(), "127.0.0.1".to_string()])
                .expect("leaf params");
        params
            .distinguished_name
            .push(DnType::CommonName, common_name);
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyEncipherment,
        ];
        params.extended_key_usages = vec![
            ExtendedKeyUsagePurpose::ServerAuth,
            ExtendedKeyUsagePurpose::ClientAuth,
        ];
        params.use_authority_key_identifier_extension = true;
        let cert = params
            .signed_by(&key, &self.cert, &self.key)
            .expect("sign leaf");
        (cert.pem(), key.serialize_pem())
    }
}

/// PEM material on disk, in the shape `HttpTransportConfig` expects.
struct NodePki {
    _dir: TempDir,
    cert: PathBuf,
    key: PathBuf,
    ca: PathBuf,
    cert_der: Vec<CertificateDer<'static>>,
    key_der: PrivateKeyDer<'static>,
}

/// Write a node's PEM bundle: a leaf signed by `issuer`, trusting `trusted`.
///
/// Passing two different CAs is how the negative tests build a peer whose
/// identity or trust anchor does not line up with its counterparty.
fn write_node_pki(name: &str, issuer: &TestCa, trusted: &TestCa) -> NodePki {
    let dir = tempfile::tempdir().expect("tempdir");
    let (cert_pem, key_pem) = issuer.issue_leaf(name);
    let ca_pem = trusted.cert.pem();

    let cert = dir.path().join("cert.pem");
    let key = dir.path().join("key.pem");
    let ca = dir.path().join("ca.pem");
    std::fs::write(&cert, &cert_pem).expect("write cert");
    std::fs::write(&key, &key_pem).expect("write key");
    std::fs::write(&ca, &ca_pem).expect("write ca");

    let cert_der = rustls_pemfile::certs(&mut cert_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .expect("parse leaf cert DER");
    let key_der = rustls_pemfile::private_key(&mut key_pem.as_bytes())
        .expect("parse leaf key DER")
        .expect("leaf key present");

    NodePki {
        _dir: dir,
        cert,
        key,
        ca,
        cert_der,
        key_der,
    }
}

// ── Harness ───────────────────────────────────────────────────────

/// Install the post-quantum-preferring rustls provider once per test binary.
///
/// `HttpTransport` reads the process-global provider for both its listener
/// and its `reqwest` client, so this must run before the first config is
/// built or the PQ group would never be offered.
fn install_pq_provider() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        springtale_transport::crypto_provider::install_default_pq();
    });
}

/// Reserve an ephemeral loopback port. `HttpTransport` takes an address
/// string and never reports the port it actually bound, so the port has to
/// be chosen before `bind()`.
fn reserve_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve port");
    let port = listener.local_addr().expect("local_addr").port();
    drop(listener);
    port
}

fn node_id(seed: u8) -> NodeId {
    NodeId::from_bytes([seed; 32])
}

fn config_for(pki: &NodePki, port: u16, peers: &[(&NodeId, u16)]) -> HttpTransportConfig {
    let peers = peers
        .iter()
        .map(|(id, peer_port)| (hex::encode(id.as_bytes()), format!("127.0.0.1:{peer_port}")))
        .collect::<HashMap<_, _>>();

    // `HttpTransportConfig` is `Deserialize`-only (config structs never derive
    // `Serialize`), so build it through serde rather than a struct literal.
    serde_json::from_value(serde_json::json!({
        "listen_addr": format!("127.0.0.1:{port}"),
        "tls_cert": pki.cert,
        "tls_key": pki.key,
        "tls_ca": pki.ca,
        "peers": peers,
    }))
    .expect("build HttpTransportConfig")
}

/// Poll until the transport's listener accepts TCP, so tests never race the
/// spawned `axum_server` task.
async fn wait_until_listening(port: u16) {
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().expect("addr");
    for _ in 0..200 {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("transport never began listening on {addr}");
}

fn message(payload: &[u8]) -> Message {
    Message {
        id: Uuid::new_v4(),
        payload: payload.to_vec(),
    }
}

// ── Raw rustls probe ──────────────────────────────────────────────

#[derive(Debug)]
enum ProbeError {
    Io(std::io::Error),
    Tls(rustls::Error),
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "probe I/O error ({:?}): {err}", err.kind()),
            Self::Tls(err) => write!(f, "probe TLS error: {err}"),
        }
    }
}

/// What a completed probe observed about the connection.
struct ProbeOutcome {
    kx_group: Option<NamedGroup>,
    /// Outcome of exchanging application data once the handshake finished.
    ///
    /// In TLS 1.3 the client finishes its handshake before the server has
    /// validated the client certificate, so a rejected client cert shows up
    /// here as a fatal alert rather than as a handshake error.
    app_data: Result<(), ProbeError>,
}

/// Handshake with `addr` as a plain rustls client, trusting `roots` and
/// optionally presenting `identity`.
///
/// Returns `Err` when the *handshake* fails, carrying the typed
/// [`rustls::Error`] so callers can assert the precise reason.
fn tls_probe(
    port: u16,
    roots: &[CertificateDer<'static>],
    identity: Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)>,
) -> Result<ProbeOutcome, ProbeError> {
    let mut root_store = RootCertStore::empty();
    for root in roots {
        root_store.add(root.clone()).expect("add probe root");
    }

    let builder = ClientConfig::builder().with_root_certificates(root_store);
    let mut config = match identity {
        Some((certs, key)) => builder
            .with_client_auth_cert(certs, key)
            .expect("probe client auth cert"),
        None => builder.with_no_client_auth(),
    };
    config.alpn_protocols = vec![b"http/1.1".to_vec()];

    let server_name = ServerName::try_from("127.0.0.1").expect("probe server name");
    let mut conn =
        ClientConnection::new(Arc::new(config), server_name).expect("probe client connection");

    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().expect("probe addr");
    let mut sock = TcpStream::connect(addr).map_err(ProbeError::Io)?;
    sock.set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(ProbeError::Io)?;

    drive_handshake(&mut conn, &mut sock)?;

    let kx_group = conn.negotiated_key_exchange_group().map(|g| g.name());
    let app_data = exchange_app_data(&mut conn, &mut sock);

    Ok(ProbeOutcome { kx_group, app_data })
}

/// Flush every byte rustls has queued for the wire.
fn flush_out(conn: &mut ClientConnection, sock: &mut TcpStream) -> Result<(), ProbeError> {
    while conn.wants_write() {
        conn.write_tls(sock).map_err(ProbeError::Io)?;
    }
    sock.flush().map_err(ProbeError::Io)
}

/// Pull one TLS record flight off the socket and process it, surfacing rustls
/// failures with their real type instead of an opaque `io::Error`.
fn pump_in(conn: &mut ClientConnection, sock: &mut TcpStream) -> Result<usize, ProbeError> {
    let read = conn.read_tls(sock).map_err(ProbeError::Io)?;
    conn.process_new_packets().map_err(ProbeError::Tls)?;
    Ok(read)
}

fn eof() -> ProbeError {
    ProbeError::Io(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "peer closed the connection",
    ))
}

fn drive_handshake(conn: &mut ClientConnection, sock: &mut TcpStream) -> Result<(), ProbeError> {
    loop {
        flush_out(conn, sock)?;
        if !conn.is_handshaking() {
            return Ok(());
        }
        if pump_in(conn, sock)? == 0 {
            return Err(eof());
        }
    }
}

/// Send a minimal request and read until the server answers or rejects us.
fn exchange_app_data(conn: &mut ClientConnection, sock: &mut TcpStream) -> Result<(), ProbeError> {
    conn.writer()
        .write_all(b"GET /transport/send HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .map_err(ProbeError::Io)?;

    let mut plaintext = Vec::new();
    loop {
        flush_out(conn, sock)?;

        let mut buf = [0u8; 4096];
        loop {
            match conn.reader().read(&mut buf) {
                Ok(0) => break,
                Ok(n) => plaintext.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(ProbeError::Io(e)),
            }
        }
        if !plaintext.is_empty() {
            return Ok(());
        }

        if pump_in(conn, sock)? == 0 {
            return Err(eof());
        }
    }
}

// ── 1. Round trip ─────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_http_transport_round_trip_delivers_message_both_ways() {
    install_pq_provider();
    let ca = TestCa::new("springtale round-trip CA");

    let (id_a, id_b) = (node_id(0xa1), node_id(0xb2));
    let (port_a, port_b) = (reserve_port(), reserve_port());

    let pki_a = write_node_pki("node-a", &ca, &ca);
    let pki_b = write_node_pki("node-b", &ca, &ca);

    let node_a = HttpTransport::bind(id_a, config_for(&pki_a, port_a, &[(&id_b, port_b)]))
        .await
        .expect("bind node A");
    let node_b = HttpTransport::bind(id_b, config_for(&pki_b, port_b, &[(&id_a, port_a)]))
        .await
        .expect("bind node B");
    wait_until_listening(port_a).await;
    wait_until_listening(port_b).await;

    assert_eq!(node_a.name(), "http");
    assert_eq!(node_a.node_id(), &id_a);

    // A → B
    let outbound = message(b"colony ping");
    node_a
        .send(&id_b, outbound.clone())
        .await
        .expect("A sends to B over mTLS");

    let (sender, received) = tokio::time::timeout(Duration::from_secs(10), node_b.recv())
        .await
        .expect("B receives before timeout")
        .expect("B receives without transport error");
    assert_eq!(sender, id_a);
    assert_eq!(received.id, outbound.id);
    assert_eq!(received.payload, b"colony ping".to_vec());

    // B → A, over the same mutually-authenticated trust anchor.
    let reply = message(b"colony pong");
    node_b
        .send(&id_a, reply.clone())
        .await
        .expect("B sends to A over mTLS");

    let (sender, received) = tokio::time::timeout(Duration::from_secs(10), node_a.recv())
        .await
        .expect("A receives before timeout")
        .expect("A receives without transport error");
    assert_eq!(sender, id_b);
    assert_eq!(received.id, reply.id);
    assert_eq!(received.payload, b"colony pong".to_vec());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_http_transport_send_to_unknown_peer_is_rejected() {
    install_pq_provider();
    let ca = TestCa::new("springtale unknown-peer CA");
    let id_a = node_id(0x11);
    let port_a = reserve_port();
    let pki_a = write_node_pki("node-a", &ca, &ca);

    let node_a = HttpTransport::bind(id_a, config_for(&pki_a, port_a, &[]))
        .await
        .expect("bind node A");

    let stranger = node_id(0x99);
    let err = node_a
        .send(&stranger, message(b"nobody home"))
        .await
        .expect_err("unrouted peer must not be dialled");

    match err {
        TransportError::ConnectionFailed(msg) => {
            assert!(
                msg.contains("unknown peer") && msg.contains(&hex::encode(stranger.as_bytes())),
                "expected an unknown-peer rejection naming the node id, got: {msg}"
            );
        }
        other => panic!("expected ConnectionFailed, got {other:?}"),
    }
}

// ── 2. Wrong certificate authority ────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_http_transport_client_trusting_wrong_ca_is_refused() {
    install_pq_provider();
    let good_ca = TestCa::new("springtale good CA");
    let evil_ca = TestCa::new("springtale evil CA");

    let (id_server, id_client) = (node_id(0x51), node_id(0x52));
    let (port_server, port_client) = (reserve_port(), reserve_port());

    // Server: identity and trust anchor both from the good CA.
    let pki_server = write_node_pki("server", &good_ca, &good_ca);
    // Client: issued by the good CA (so its own cert is acceptable), but its
    // trust store holds only the evil CA — it cannot verify the server.
    let pki_client = write_node_pki("client", &good_ca, &evil_ca);

    let server = HttpTransport::bind(
        id_server,
        config_for(&pki_server, port_server, &[(&id_client, port_client)]),
    )
    .await
    .expect("bind server");
    let client = HttpTransport::bind(
        id_client,
        config_for(&pki_client, port_client, &[(&id_server, port_server)]),
    )
    .await
    .expect("bind client");
    wait_until_listening(port_server).await;

    let err = client
        .send(&id_server, message(b"should never arrive"))
        .await
        .expect_err("server certificate signed by an untrusted CA must be refused");
    match err {
        TransportError::Http(msg) => {
            assert!(
                msg.contains(&format!("127.0.0.1:{port_server}")),
                "expected the transport error to name the peer, got: {msg}"
            );
        }
        other => panic!("expected a TLS-layer Http error, got {other:?}"),
    }

    // Nothing reached the server's inbox.
    assert!(
        tokio::time::timeout(Duration::from_millis(500), server.recv())
            .await
            .is_err(),
        "a message crossed a connection that should have failed to handshake"
    );

    // Pin the exact rustls reason against the same live server: trusting only
    // the evil CA must fail server-certificate verification with
    // `UnknownIssuer`. If verification were disabled this handshake would
    // succeed and the test would fail here.
    let probe = tokio::task::spawn_blocking({
        let roots = vec![evil_ca.der()];
        move || tls_probe(port_server, &roots, None)
    })
    .await
    .expect("probe task");

    match probe {
        Err(ProbeError::Tls(rustls::Error::InvalidCertificate(
            CertificateError::UnknownIssuer,
        ))) => {}
        Err(other) => panic!("expected InvalidCertificate(UnknownIssuer), got {other:?}"),
        Ok(_) => panic!("handshake succeeded against an untrusted server certificate"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_http_transport_client_cert_from_wrong_ca_is_refused() {
    install_pq_provider();
    let good_ca = TestCa::new("springtale good CA");
    let evil_ca = TestCa::new("springtale evil CA");

    let (id_server, id_client) = (node_id(0x61), node_id(0x62));
    let (port_server, port_client) = (reserve_port(), reserve_port());

    let pki_server = write_node_pki("server", &good_ca, &good_ca);
    // Client trusts the good CA (so the server's certificate verifies), but
    // presents an identity the server's `WebPkiClientVerifier` cannot chain.
    let pki_client = write_node_pki("client", &evil_ca, &good_ca);

    let server = HttpTransport::bind(
        id_server,
        config_for(&pki_server, port_server, &[(&id_client, port_client)]),
    )
    .await
    .expect("bind server");
    let client = HttpTransport::bind(
        id_client,
        config_for(&pki_client, port_client, &[(&id_server, port_server)]),
    )
    .await
    .expect("bind client");
    wait_until_listening(port_server).await;

    let err = client
        .send(&id_server, message(b"forged identity"))
        .await
        .expect_err("client certificate from an untrusted CA must be refused");
    assert!(
        matches!(err, TransportError::Http(_)),
        "expected a TLS-layer Http error, got {err:?}"
    );

    assert!(
        tokio::time::timeout(Duration::from_millis(500), server.recv())
            .await
            .is_err(),
        "a message crossed a connection whose client certificate was untrusted"
    );

    // Pin the reason. TLS 1.3 clients finish their side of the handshake
    // before the server validates the client certificate, so the rejection
    // arrives as a fatal alert on the first application-data exchange.
    let probe = tokio::task::spawn_blocking({
        let roots = vec![good_ca.der()];
        let certs = pki_client.cert_der.clone();
        let key = pki_client.key_der.clone_key();
        move || tls_probe(port_server, &roots, Some((certs, key)))
    })
    .await
    .expect("probe task")
    .expect("server certificate verifies for this probe");

    match probe.app_data {
        Err(ProbeError::Tls(rustls::Error::AlertReceived(alert))) => {
            assert!(
                matches!(
                    alert,
                    rustls::AlertDescription::UnknownCA
                        | rustls::AlertDescription::BadCertificate
                        | rustls::AlertDescription::DecryptError
                        | rustls::AlertDescription::CertificateUnknown
                ),
                "expected a certificate-rejection alert, got {alert:?}"
            );
        }
        Err(other) => panic!("expected a fatal certificate alert, got {other:?}"),
        Ok(()) => panic!("server accepted a client certificate from an untrusted CA"),
    }
}

// ── 3. Post-quantum key exchange ──────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_http_transport_negotiates_x25519mlkem768() {
    install_pq_provider();
    let ca = TestCa::new("springtale pq CA");

    let (id_server, id_client) = (node_id(0x71), node_id(0x72));
    let port_server = reserve_port();

    let pki_server = write_node_pki("server", &ca, &ca);
    let pki_client = write_node_pki("client", &ca, &ca);

    let _server = HttpTransport::bind(
        id_server,
        config_for(&pki_server, port_server, &[(&id_client, port_server)]),
    )
    .await
    .expect("bind server");
    wait_until_listening(port_server).await;

    let probe = tokio::task::spawn_blocking({
        let roots = vec![ca.der()];
        let certs = pki_client.cert_der.clone();
        let key = pki_client.key_der.clone_key();
        move || tls_probe(port_server, &roots, Some((certs, key)))
    })
    .await
    .expect("probe task")
    .expect("mTLS handshake with matching CA succeeds");

    // Asserted from the completed connection's handshake state, not from the
    // configured `kx_groups` list.
    assert_eq!(
        probe.kx_group,
        Some(NamedGroup::X25519MLKEM768),
        "transport must negotiate the hybrid post-quantum group (NIST IR 8547)"
    );
    assert!(
        probe.app_data.is_ok(),
        "post-handshake exchange failed: {:?}",
        probe.app_data
    );
}
