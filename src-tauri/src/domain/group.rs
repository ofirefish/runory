use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostGroup {
    pub id: Uuid,
    pub name: String,
    pub sort_order: i32,
    pub collapsed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateGroupRequest {
    pub name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupRequest {
    pub id: Uuid,
    pub name: String,
    pub sort_order: i32,
    pub collapsed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteGroupRequest {
    pub id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderGroupsRequest {
    pub ordered_ids: Vec<Uuid>,
}
