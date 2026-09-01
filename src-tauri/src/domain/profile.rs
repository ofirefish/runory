use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub group_id: Option<Uuid>,
    pub auth_method: AuthMethod,
    pub key_source: Option<KeySource>,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
    pub last_connected_at: Option<String>,
    #[serde(default)]
    pub os_distribution: Option<OsDistribution>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OsDistribution {
    Ubuntu,
    Debian,
    Fedora,
    Centos,
    RedHat,
    RockyLinux,
    AlmaLinux,
    ArchLinux,
    Manjaro,
    OpenSuse,
    AlpineLinux,
    AmazonLinux,
    OracleLinux,
    LinuxMint,
    KaliLinux,
    Gentoo,
    VoidLinux,
    NixOs,
    MacOs,
    FreeBsd,
    Linux,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    Password,
    PrivateKey,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum KeySource {
    File { path: String },
    Vault { key_id: Uuid },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProfileRequest {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub group_id: Option<Uuid>,
    pub auth_method: AuthMethod,
    pub key_source: Option<KeySource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileRequest {
    pub id: Uuid,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub group_id: Option<Uuid>,
    pub auth_method: AuthMethod,
    pub key_source: Option<KeySource>,
    pub sort_order: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProfileRequest {
    pub id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderProfilesRequest {
    pub group_id: Option<Uuid>,
    pub ordered_ids: Vec<Uuid>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_schema_cannot_serialize_password() {
        let profile = ServerProfile {
            id: Uuid::new_v4(),
            name: "test".into(),
            host: "localhost".into(),
            port: 22,
            username: "runory".into(),
            group_id: None,
            auth_method: AuthMethod::Password,
            key_source: None,
            sort_order: 0,
            created_at: "now".into(),
            updated_at: "now".into(),
            last_connected_at: None,
            os_distribution: None,
        };
        let json = serde_json::to_string(&profile).unwrap_or_default();
        assert!(!json.contains("password\":"));
        assert!(!json.contains("passphrase"));
    }

    #[test]
    fn legacy_profile_without_os_distribution_still_deserializes() {
        let profile = serde_json::from_value::<ServerProfile>(serde_json::json!({
            "id": Uuid::new_v4(),
            "name": "legacy",
            "host": "localhost",
            "port": 22,
            "username": "runory",
            "groupId": null,
            "authMethod": "password",
            "keySource": null,
            "sortOrder": 0,
            "createdAt": "now",
            "updatedAt": "now",
            "lastConnectedAt": null
        }))
        .expect("deserialize legacy profile");
        assert_eq!(profile.os_distribution, None);
    }
}
