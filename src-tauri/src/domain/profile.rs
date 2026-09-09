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
    #[serde(default)]
    pub connection_route: ConnectionRoute,
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

impl OsDistribution {
    pub fn display_label(self) -> &'static str {
        match self {
            Self::Ubuntu => "Ubuntu",
            Self::Debian => "Debian",
            Self::Fedora => "Fedora",
            Self::Centos => "CentOS",
            Self::RedHat => "Red Hat",
            Self::RockyLinux => "Rocky Linux",
            Self::AlmaLinux => "AlmaLinux",
            Self::ArchLinux => "Arch Linux",
            Self::Manjaro => "Manjaro",
            Self::OpenSuse => "openSUSE",
            Self::AlpineLinux => "Alpine Linux",
            Self::AmazonLinux => "Amazon Linux",
            Self::OracleLinux => "Oracle Linux",
            Self::LinuxMint => "Linux Mint",
            Self::KaliLinux => "Kali Linux",
            Self::Gentoo => "Gentoo",
            Self::VoidLinux => "Void Linux",
            Self::NixOs => "NixOS",
            Self::MacOs => "macOS",
            Self::FreeBsd => "FreeBSD",
            Self::Linux => "Linux",
        }
    }
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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ConnectionRoute {
    #[default]
    Direct,
    JumpHost {
        #[serde(rename = "profileId", alias = "profile_id")]
        profile_id: Uuid,
    },
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
    #[serde(default)]
    pub connection_route: ConnectionRoute,
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
    #[serde(default)]
    pub connection_route: ConnectionRoute,
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
            connection_route: ConnectionRoute::Direct,
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

    #[test]
    fn create_profile_request_accepts_camel_case_jump_host_profile_id() {
        let profile_id = Uuid::new_v4();
        let request = serde_json::from_value::<CreateProfileRequest>(serde_json::json!({
            "name": "Target",
            "host": "target.internal",
            "port": 22,
            "username": "deploy",
            "groupId": null,
            "authMethod": "password",
            "keySource": null,
            "connectionRoute": {
                "type": "jumpHost",
                "profileId": profile_id,
            },
        }))
        .expect("deserialize create-profile request from the frontend");

        assert_eq!(
            request.connection_route,
            ConnectionRoute::JumpHost { profile_id }
        );
        assert_eq!(
            serde_json::to_value(&request.connection_route).expect("serialize jump-host route"),
            serde_json::json!({
                "type": "jumpHost",
                "profileId": profile_id,
            })
        );
    }

    #[test]
    fn jump_host_route_accepts_the_existing_snake_case_storage_field() {
        let profile_id = Uuid::new_v4();
        let route = serde_json::from_value::<ConnectionRoute>(serde_json::json!({
            "type": "jumpHost",
            "profile_id": profile_id,
        }))
        .expect("deserialize stored jump-host route");

        assert_eq!(route, ConnectionRoute::JumpHost { profile_id });
    }
}
