use super::client::MarketplaceClient;
use crate::plugins::error::PluginError;
use crate::plugins::installer::package::MAX_PACKAGE_BYTES;
use crate::plugins::registry::PluginRegistryRepository;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn server(response: Vec<u8>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listen");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        let mut request = [0u8; 4096];
        let _ = stream.read(&mut request).await;
        let _ = stream.write_all(&response).await;
    });
    format!("http://{address}")
}

fn client(url: Option<String>) -> MarketplaceClient {
    MarketplaceClient::new(
        Arc::new(PluginRegistryRepository::open_in_memory().expect("repository")),
        url,
    )
}

fn response(body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

#[tokio::test]
async fn download_requires_hash_and_never_persists_mismatch() {
    let dir = tempfile::tempdir().expect("temp");
    let dest = dir.path().join("package");
    let url = server(response(b"downloaded package")).await;
    assert!(matches!(
        client(None)
            .download_package(&url, &"0".repeat(64), &dest)
            .await,
        Err(PluginError::HashMismatch)
    ));
    assert!(!dest.exists());
    assert!(matches!(
        client(None)
            .download_package("http://127.0.0.1:1", "", &dest)
            .await,
        Err(PluginError::HashMismatch)
    ));
}

#[tokio::test]
async fn download_rejects_oversize_before_reading_body() {
    let dir = tempfile::tempdir().expect("temp");
    let dest = dir.path().join("package");
    let url = server(
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_PACKAGE_BYTES + 1
        )
        .into_bytes(),
    )
    .await;
    assert!(matches!(
        client(None)
            .download_package(&url, &"0".repeat(64), &dest)
            .await,
        Err(PluginError::InvalidPackage)
    ));
    assert!(!dest.exists());
}

#[tokio::test]
async fn verified_download_persists_exact_bytes() {
    let dir = tempfile::tempdir().expect("temp");
    let bytes = b"downloaded package";
    let hash = ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let url = server(response(bytes)).await;
    let dest = dir.path().join("package");
    client(None)
        .download_package(&url, &hash, &dest)
        .await
        .expect("download");
    assert_eq!(std::fs::read(dest).expect("read"), bytes);
}

#[tokio::test]
async fn catalog_identity_and_status_are_checked_before_download() {
    for (id, status) in [("../../escape", "published"), ("runory.review", "revoked")] {
        let body = serde_json::to_vec(&serde_json::json!([{
            "pluginId": id, "version": "1.0.0", "apiVersion": "1", "minRunoryVersion": null,
            "packageSha256": "0".repeat(64), "downloadUrl": "/package", "status": status,
            "changelog": null, "permissions": []
        }]))
        .expect("json");
        let url = server(response(&body)).await;
        let dir = tempfile::tempdir().expect("temp");
        assert!(matches!(
            client(Some(url))
                .download_and_stage("runory.review", Some("1.0.0"), dir.path())
                .await,
            Err(PluginError::InvalidPackage)
        ));
        assert_eq!(std::fs::read_dir(dir.path()).expect("files").count(), 0);
    }
}
