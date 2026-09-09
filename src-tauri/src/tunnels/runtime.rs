use super::model::{now, TunnelHealth, TunnelRule, TunnelState, TunnelStatus};
use crate::{
    domain::{AppError, AppResult},
    ssh::{BackgroundConnection, ForwardTransport},
};
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::{TcpListener, TcpStream},
    sync::{watch, Mutex},
    task::{JoinHandle, JoinSet},
};

#[derive(Default)]
pub struct Counters {
    active: AtomicU64,
    sent: AtomicU64,
    received: AtomicU64,
}

pub struct LiveTunnel {
    pub status: Arc<Mutex<TunnelStatus>>,
    pub transport: ForwardTransport,
    pub stop: watch::Sender<bool>,
    counters: Arc<Counters>,
    task: Option<JoinHandle<()>>,
}

impl LiveTunnel {
    pub fn start_background(
        rule: TunnelRule,
        owner: Arc<BackgroundConnection>,
        listener: TcpListener,
    ) -> Self {
        Self::start_owned(rule, owner.id, owner.transport(), listener, Some(owner))
    }
    #[cfg(test)]
    pub fn start(
        rule: TunnelRule,
        session_id: uuid::Uuid,
        transport: ForwardTransport,
        listener: TcpListener,
    ) -> Self {
        Self::start_owned(rule, session_id, transport, listener, None)
    }
    fn start_owned(
        rule: TunnelRule,
        session_id: uuid::Uuid,
        transport: ForwardTransport,
        listener: TcpListener,
        owner: Option<Arc<BackgroundConnection>>,
    ) -> Self {
        let mut initial = TunnelStatus {
            state: TunnelState::Running,
            session_id: Some(session_id),
            started_at: Some(now()),
            ..Default::default()
        };
        initial.event("started");
        let status = Arc::new(Mutex::new(initial));
        let counters = Arc::new(Counters::default());
        let (stop, receiver) = watch::channel(false);
        let serving = serve(
            rule,
            transport.clone(),
            listener,
            receiver,
            status.clone(),
            counters.clone(),
        );
        let task = tokio::spawn(async move {
            serving.await;
            drop(owner);
        });
        Self {
            status,
            transport,
            stop,
            counters,
            task: Some(task),
        }
    }
    pub async fn snapshot(&self) -> TunnelStatus {
        let mut status = self.status.lock().await.clone();
        status.active_connections = self.counters.active.load(Ordering::Relaxed);
        status.bytes_sent = self.counters.sent.load(Ordering::Relaxed);
        status.bytes_received = self.counters.received.load(Ordering::Relaxed);
        status
    }
    pub async fn shutdown(&mut self) {
        let _ = self.stop.send(true);
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}
impl Drop for LiveTunnel {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

async fn serve(
    rule: TunnelRule,
    transport: ForwardTransport,
    listener: TcpListener,
    mut stop: watch::Receiver<bool>,
    status: Arc<Mutex<TunnelStatus>>,
    counters: Arc<Counters>,
) {
    let mut streams = JoinSet::new();
    let mut tick = tokio::time::interval(Duration::from_millis(200));
    let (end_state, end_code) = loop {
        tokio::select! {
            biased;
            _ = stop.changed() => break (TunnelState::Stopped, "stopped"),
            _ = tick.tick() => {
                if transport.is_closed() { break (TunnelState::Interrupted, "CONNECTION_LOST"); }
            }
            Some(_) = streams.join_next(), if !streams.is_empty() => {}
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else { break (TunnelState::Error, "TUNNEL_BIND_FAILED"); };
                if streams.len() >= 64 {
                    status.lock().await.event("TUNNEL_LIMIT");
                    continue;
                }
                let transport = transport.clone();
                let rule = rule.clone();
                let status = status.clone();
                let counters = counters.clone();
                streams.spawn(async move {
                    let _connection = ActiveConnection::new(counters.clone());
                    match transport.open(&rule.target_host, rule.target_port, peer.port()).await {
                        Ok(channel) => {
                            let mut ssh = channel.into_stream();
                            let mut local = CountedStream { stream, counters };
                            if tokio::io::copy_bidirectional(&mut local, &mut ssh).await.is_err() {
                                status.lock().await.event("stream-failed");
                            }
                        }
                        Err(error) => {
                            let mut current = status.lock().await;
                            current.error_code = Some(error.code().into());
                            current.event(error.code());
                        }
                    }
                });
            }
        }
    };
    // Stop completion means all accepted streams and the listener are actually released.
    drop(listener);
    streams.abort_all();
    while streams.join_next().await.is_some() {}
    let mut current = status.lock().await;
    current.state = end_state;
    current.health = TunnelHealth::Unchecked;
    current.checked_at = None;
    current.error_code = (end_state != TunnelState::Stopped).then(|| end_code.into());
    current.event(end_code);
}

pub async fn probe(
    rule: &TunnelRule,
    transport: ForwardTransport,
    status: Arc<Mutex<TunnelStatus>>,
    mut stop: watch::Receiver<bool>,
) -> AppResult<()> {
    if *stop.borrow() {
        return Err(AppError::TunnelStopped);
    }
    let result = tokio::select! {
        biased;
        _ = stop.changed() => return Err(AppError::TunnelStopped),
        result = transport.open(&rule.target_host, rule.target_port, 0) => result,
    };
    let outcome = match result {
        Ok(channel) => {
            drop(channel.into_stream());
            Ok(())
        }
        Err(error) => Err(error),
    };
    let mut current = status.lock().await;
    if current.state != TunnelState::Running || *stop.borrow() {
        return Err(AppError::TunnelStopped);
    }
    current.checked_at = Some(now());
    current.health = if outcome.is_ok() {
        TunnelHealth::Reachable
    } else {
        TunnelHealth::Unreachable
    };
    current.error_code = outcome.as_ref().err().map(|error| error.code().into());
    current.event(
        outcome
            .as_ref()
            .err()
            .map(|error| error.code())
            .unwrap_or("reachable"),
    );
    // Unreachable is a probe result, not a listener start failure.
    Ok(())
}

struct ActiveConnection(Arc<Counters>);
impl ActiveConnection {
    fn new(counters: Arc<Counters>) -> Self {
        counters.active.fetch_add(1, Ordering::Relaxed);
        Self(counters)
    }
}
impl Drop for ActiveConnection {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::Relaxed);
    }
}

struct CountedStream {
    stream: TcpStream,
    counters: Arc<Counters>,
}
impl AsyncRead for CountedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buffer.filled().len();
        let result = Pin::new(&mut self.stream).poll_read(cx, buffer);
        self.counters
            .sent
            .fetch_add((buffer.filled().len() - before) as u64, Ordering::Relaxed);
        result
    }
}
impl AsyncWrite for CountedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let result = Pin::new(&mut self.stream).poll_write(cx, data);
        if let Poll::Ready(Ok(count)) = &result {
            self.counters
                .received
                .fetch_add(*count as u64, Ordering::Relaxed);
        }
        result
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}
