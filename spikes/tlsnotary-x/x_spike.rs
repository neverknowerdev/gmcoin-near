//! Spike: can a TLSNotary prover (MPC-TLS) notarize responses from x.com hosts,
//! and how long / how many bytes does it take?
//!
//! Runs prover and verifier in one process, over an in-memory channel that
//! counts the bytes exchanged between them. The prover talks to the real server.
//!
//! Env:
//!   TARGET_URL  https URL to fetch (GET unless METHOD is set)
//!   METHOD      GET (default) or POST
//!   HEADERS     extra request headers, one "Name: value" per line
//!   HIDE        comma-separated header names whose values stay hidden
//!   MAX_SENT    bytes the prover may send to the server (default 4096)
//!   MAX_RECV    bytes the prover may receive from the server (default 16384)

use std::{
    env,
    future::IntoFuture,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    task::{Context, Poll},
    time::Instant,
};

use anyhow::{Result, anyhow};
use http_body_util::{BodyExt, Empty};
use hyper::{Request, Uri, body::Bytes};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use tlsn::{
    Session,
    config::{
        prove::ProveConfig, prover::ProverConfig, tls::TlsClientConfig,
        tls_commit::mpc::MpcTlsConfig, verifier::VerifierConfig,
    },
    connection::ServerName,
    transcript::PartialTranscript,
    verifier::{VerifierCommitStart, VerifierOutput},
    webpki::RootCertStore,
};

/// Counts bytes read from and written to the wrapped stream.
struct Counting<T> {
    inner: T,
    read: Arc<AtomicU64>,
    written: Arc<AtomicU64>,
}

impl<T: AsyncRead + Unpin> AsyncRead for Counting<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let res = Pin::new(&mut self.inner).poll_read(cx, buf);
        let n = (buf.filled().len() - before) as u64;
        self.read.fetch_add(n, Ordering::Relaxed);
        res
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for Counting<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let res = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = &res {
            self.written.fetch_add(*n as u64, Ordering::Relaxed);
        }
        res
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1_000_000.0
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let url = env::var("TARGET_URL").expect("TARGET_URL is required");
    let max_sent = env_usize("MAX_SENT", 1 << 12);
    let max_recv = env_usize("MAX_RECV", 1 << 14);

    let (prover_socket, verifier_socket) = tokio::io::duplex(1 << 23);
    let up = Arc::new(AtomicU64::new(0));
    let down = Arc::new(AtomicU64::new(0));
    let prover_socket = Counting {
        inner: prover_socket,
        read: down.clone(),
        written: up.clone(),
    };

    let started = Instant::now();
    let (_, transcript) = tokio::try_join!(
        prover(prover_socket, &url, max_sent, max_recv, started),
        verifier(verifier_socket, max_sent, max_recv)
    )?;
    let total = started.elapsed();

    let received = transcript.received_unsafe(transcript.len_received()).unwrap();
    let text = String::from_utf8_lossy(&received);
    let status_line = text.lines().next().unwrap_or("").to_string();
    let preview: String = text
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .chars()
        .take(400)
        .collect();

    println!("RESULT url={url}");
    println!("RESULT status_line={status_line}");
    println!(
        "RESULT sent_bytes={} recv_bytes={}",
        transcript.len_sent(),
        transcript.len_received()
    );
    println!("RESULT max_sent={max_sent} max_recv={max_recv}");
    println!(
        "RESULT prover_upload_mb={:.1} prover_download_mb={:.1}",
        mb(up.load(Ordering::Relaxed)),
        mb(down.load(Ordering::Relaxed))
    );
    println!("RESULT total_seconds={:.1}", total.as_secs_f64());
    println!("RESULT body_preview={}", preview.replace('\n', " "));
    Ok(())
}

async fn prover<T: AsyncWrite + AsyncRead + Send + Unpin + 'static>(
    verifier_socket: T,
    url: &str,
    max_sent: usize,
    max_recv: usize,
    started: Instant,
) -> Result<()> {
    let uri = url.parse::<Uri>()?;
    let host = uri.host().ok_or_else(|| anyhow!("url has no host"))?.to_string();
    let method = env::var("METHOD").unwrap_or_else(|_| "GET".into());
    let hide: Vec<String> = env::var("HIDE")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let session = Session::new(verifier_socket.compat());
    let (driver, mut handle) = session.split();
    let driver_task = tokio::spawn(driver);

    let prover = handle
        .new_prover(ProverConfig::builder().build()?)?
        .commit(
            MpcTlsConfig::builder()
                .max_sent_data(max_sent)
                .max_recv_data(max_recv)
                .build()?,
        )
        .await?;
    println!("TIMING preprocess_done={:.1}s", started.elapsed().as_secs_f64());

    let client_socket = tokio::net::TcpStream::connect((host.as_str(), 443)).await?;
    client_socket.set_nodelay(true)?;
    println!("INFO server_ip={}", client_socket.peer_addr()?);

    let (tls_connection, prover) = prover.connect(
        TlsClientConfig::builder()
            .server_name(ServerName::Dns(host.as_str().try_into()?))
            .root_store(RootCertStore::mozilla())
            .build()?,
        client_socket.compat(),
    )?;
    let tls_connection = TokioIo::new(tls_connection.compat());
    let prover_task = tokio::spawn(prover.into_future());

    let (mut request_sender, connection) =
        hyper::client::conn::http1::handshake(tls_connection).await?;
    tokio::spawn(connection);

    let mut builder = Request::builder()
        .method(method.as_str())
        .uri(uri.path_and_query().map(|p| p.as_str()).unwrap_or("/"))
        .header("Host", host.as_str())
        .header("Connection", "close")
        .header("Accept", "*/*")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0 Safari/537.36",
        );
    for line in env::var("HEADERS").unwrap_or_default().lines() {
        if let Some((k, v)) = line.split_once(':') {
            builder = builder.header(k.trim(), v.trim());
        }
    }
    let request = builder.body(Empty::<Bytes>::new())?;
    let response = request_sender.send_request(request).await?;
    println!("INFO http_status={}", response.status());
    // Drain the body so the whole response is in the transcript.
    let _ = response.into_body().collect().await;
    println!("TIMING response_done={:.1}s", started.elapsed().as_secs_f64());

    let mut prover = prover_task.await??;
    let sent = prover.transcript().sent().to_vec();
    let recv_len = prover.transcript().received().len();

    let mut config = ProveConfig::builder(prover.transcript());
    config.server_identity();

    // Reveal the request except the values of hidden headers.
    let mut hidden: Vec<(usize, usize)> = Vec::new();
    let sent_text = String::from_utf8_lossy(&sent).to_string();
    let mut offset = 0;
    for line in sent_text.split_inclusive("\r\n") {
        if let Some((name, _)) = line.split_once(':') {
            if hide.contains(&name.trim().to_ascii_lowercase()) {
                let start = offset + name.len() + 1;
                let end = offset + line.trim_end_matches("\r\n").len();
                hidden.push((start, end));
            }
        }
        offset += line.len();
    }
    let mut cursor = 0;
    for (start, end) in &hidden {
        if *start > cursor {
            config.reveal_sent(&(cursor..*start))?;
        }
        cursor = *end;
    }
    if cursor < sent.len() {
        config.reveal_sent(&(cursor..sent.len()))?;
    }
    config.reveal_recv(&(0..recv_len))?;

    prover.prove(&config.build()?).await?;
    prover.close().await?;
    handle.close();
    driver_task.await??;
    println!("TIMING prove_done={:.1}s", started.elapsed().as_secs_f64());
    Ok(())
}

async fn verifier<T: AsyncWrite + AsyncRead + Send + Sync + Unpin + 'static>(
    socket: T,
    max_sent: usize,
    max_recv: usize,
) -> Result<PartialTranscript> {
    let session = Session::new(socket.compat());
    let (driver, mut handle) = session.split();
    let driver_task = tokio::spawn(driver);

    let verifier = handle.new_verifier(
        VerifierConfig::builder()
            .root_store(RootCertStore::mozilla())
            .build()?,
    )?;

    let verifier = match verifier.commit().await? {
        VerifierCommitStart::Mpc(verifier) => {
            let cfg = verifier.config();
            if cfg.max_sent_data() > max_sent || cfg.max_recv_data() > max_recv {
                verifier.reject(Some("limits too large")).await?;
                return Err(anyhow!("protocol configuration rejected"));
            }
            verifier.accept().await?.run().await?
        }
        VerifierCommitStart::Proxy(verifier) => {
            verifier.reject(Some("expecting MPC-TLS")).await?;
            return Err(anyhow!("protocol configuration rejected"));
        }
    };

    let verifier = verifier.verify().await?;
    if !verifier.request().server_identity() {
        let verifier = verifier.reject(Some("server name not revealed")).await?;
        verifier.close().await?;
        return Err(anyhow!("prover did not reveal the server name"));
    }

    let (
        VerifierOutput {
            server_name,
            transcript,
            ..
        },
        verifier,
    ) = verifier.accept().await?;
    verifier.close().await?;
    handle.close();
    driver_task.await??;

    let ServerName::Dns(name) = server_name.ok_or_else(|| anyhow!("no server name"))?;
    println!("VERIFIED server_name={}", name.as_str());
    transcript.ok_or_else(|| anyhow!("no transcript revealed"))
}
