use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{AppError, AppResult, SessionId};

const MIN_FLEET_TARGETS: usize = 2;
pub const MAX_FLEET_TARGETS: usize = 10;
const MAX_FLEET_ROLE_BYTES: usize = 48;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetTargetRequest {
    pub profile_id: Uuid,
    pub session_id: SessionId,
    pub role: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FleetTargetBinding {
    pub profile_id: Uuid,
    pub session_id: SessionId,
    pub role: Option<String>,
    pub ordinal: usize,
}

fn normalized_role(role: Option<&str>) -> AppResult<Option<String>> {
    let Some(role) = role else {
        return Ok(None);
    };
    let role = role.trim();
    if role.is_empty()
        || role.len() > MAX_FLEET_ROLE_BYTES
        || !role
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'.' | b'_' | b'-'))
    {
        return Err(AppError::AgentFleetTargetInvalid);
    }
    Ok(Some(role.to_ascii_lowercase()))
}

pub fn validate_fleet_target_shape(
    targets: &[FleetTargetRequest],
) -> AppResult<Vec<Option<String>>> {
    if targets.len() < MIN_FLEET_TARGETS {
        return Err(AppError::AgentFleetTargetInvalid);
    }
    if targets.len() > MAX_FLEET_TARGETS {
        return Err(AppError::AgentFleetTargetLimit);
    }
    let mut profiles = HashSet::with_capacity(targets.len());
    let mut sessions = HashSet::with_capacity(targets.len());
    let mut roles = Vec::with_capacity(targets.len());
    for target in targets {
        if !profiles.insert(target.profile_id) || !sessions.insert(target.session_id) {
            return Err(AppError::AgentFleetTargetInvalid);
        }
        roles.push(normalized_role(target.role.as_deref())?);
    }
    Ok(roles)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(profile_id: Uuid, session_id: Uuid, role: Option<&str>) -> FleetTargetRequest {
        FleetTargetRequest {
            profile_id,
            session_id,
            role: role.map(str::to_owned),
        }
    }

    #[test]
    fn exact_target_shape_normalizes_roles() {
        let roles = validate_fleet_target_shape(&[
            target(Uuid::new_v4(), Uuid::new_v4(), Some(" Source ")),
            target(Uuid::new_v4(), Uuid::new_v4(), Some("REPLICA-1")),
        ])
        .expect("valid targets");

        assert_eq!(roles, vec![Some("source".into()), Some("replica-1".into())]);
    }

    #[test]
    fn exact_target_shape_rejects_duplicate_profiles_or_sessions() {
        let profile = Uuid::new_v4();
        let session = Uuid::new_v4();
        assert!(matches!(
            validate_fleet_target_shape(&[
                target(profile, session, None),
                target(profile, Uuid::new_v4(), None),
            ]),
            Err(AppError::AgentFleetTargetInvalid)
        ));
        assert!(matches!(
            validate_fleet_target_shape(&[
                target(Uuid::new_v4(), session, None),
                target(Uuid::new_v4(), session, None),
            ]),
            Err(AppError::AgentFleetTargetInvalid)
        ));
    }

    #[test]
    fn exact_target_shape_enforces_bounds_and_role_syntax() {
        assert!(matches!(
            validate_fleet_target_shape(&[target(Uuid::new_v4(), Uuid::new_v4(), None)]),
            Err(AppError::AgentFleetTargetInvalid)
        ));
        let too_many = (0..=MAX_FLEET_TARGETS)
            .map(|_| target(Uuid::new_v4(), Uuid::new_v4(), None))
            .collect::<Vec<_>>();
        assert!(matches!(
            validate_fleet_target_shape(&too_many),
            Err(AppError::AgentFleetTargetLimit)
        ));
        assert!(matches!(
            validate_fleet_target_shape(&[
                target(Uuid::new_v4(), Uuid::new_v4(), Some("source/root")),
                target(Uuid::new_v4(), Uuid::new_v4(), None),
            ]),
            Err(AppError::AgentFleetTargetInvalid)
        ));
    }
}
