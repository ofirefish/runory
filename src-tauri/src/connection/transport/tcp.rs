use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::domain::AppResult;

use super::{map_io_connect_error, OpenedTransport, TransportContext};

pub async fn open(host: &str, port: u16, ctx: &TransportContext) -> AppResult<OpenedTransport> {
    if host.trim().is_empty() || port == 0 {
        return Err(crate::domain::AppError::InvalidProfile);
    }
    let stream = timeout(ctx.timeout(), TcpStream::connect((host, port)))
        .await
        .map_err(|_| crate::domain::AppError::ConnectionTimeout)?
        .map_err(map_io_connect_error)?;
    let _ = stream.set_nodelay(true);
    Ok(OpenedTransport {
        stream: Box::new(stream),
        cleanup: None,
        helper_ready_payload: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::transport::{TransportFactory, TransportPlan};

    #[tokio::test]
    async fn tcp_open_rejects_empty_host() {
        let plan = TransportPlan::Tcp {
            host: String::new(),
            port: 22,
        };
        let err = TransportFactory::open(&plan, &TransportContext::default())
            .await
            .expect_err("empty host");
        assert!(matches!(err, crate::domain::AppError::InvalidProfile));
    }
}
