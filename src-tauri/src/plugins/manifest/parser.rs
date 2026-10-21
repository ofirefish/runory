use super::model::PluginManifest;
use crate::plugins::error::{PluginError, PluginResult};

pub fn parse_manifest_json(bytes: &[u8]) -> PluginResult<PluginManifest> {
    serde_json::from_slice(bytes).map_err(|error| {
        PluginError::InvalidManifest(format!("manifest json parse failed: {error}"))
    })
}

pub fn parse_manifest_str(text: &str) -> PluginResult<PluginManifest> {
    parse_manifest_json(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_manifest() {
        let json = r#"{
          "schemaVersion": 1,
          "id": "runory.hello",
          "name": "Runory Hello",
          "version": "1.0.0",
          "publisher": { "id": "runory", "name": "Runory" },
          "apiVersion": "1",
          "permissions": ["host.read", "ssh.exec"],
          "contributes": {
            "commands": [{ "id": "hello.inspect", "title": "Inspect Host" }],
            "workflows": [{ "id": "hello.inspect", "file": "workflows/hello.yaml" }]
          }
        }"#;
        let manifest = parse_manifest_str(json).expect("parse");
        assert_eq!(manifest.id, "runory.hello");
        assert_eq!(manifest.permissions.len(), 2);
    }
}
