use std::time::Duration;

use futures_util::stream::{self, StreamExt};
use serde::Deserialize;
use serde_json::Value;

use crate::domain::{
    AppError, AppResult, DockerContainer, DockerImage, DockerImageAction, DockerNetwork,
    DockerNetworkAction, DockerOnlineImage, DockerVolume, DockerVolumeAction, LogSource,
    NginxAction, OperationResult, Pm2Process, ResourceAction, SessionId,
};
use crate::ssh::{RemoteCommand, RemoteExecResult, ServerSessionManager};

const DOCKER_LIST: &str =
    "command -v docker >/dev/null 2>&1 || exit 90; docker ps -a --no-trunc --format '{{json .}}'";
const DOCKER_STATS: &str =
    "command -v docker >/dev/null 2>&1 || exit 90; docker stats --no-stream --format '{{json .}}'";
const DOCKER_IMAGES: &str =
    "command -v docker >/dev/null 2>&1 || exit 90; docker images --no-trunc --format '{{json .}}'";
/// Inspect all networks as JSON lines. Empty host has no ids — exit cleanly.
const DOCKER_NETWORKS: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
ids=$(docker network ls -q)
[ -z "$ids" ] && exit 0
# Intentional unquoted expansion: one network id per word for inspect.
docker network inspect $ids --format '{{json .}}'
"#;
/// Search Docker Hub from the local client. Remote `docker search` is unreliable
/// (Hub endpoint churn, remote network/mirrors) and is not used for Online images.
const DOCKER_HUB_SEARCH_URL: &str = "https://hub.docker.com/v2/search/repositories/";
const DOCKER_HUB_LIBRARY_URL: &str = "https://hub.docker.com/v2/repositories/library/";
const DOCKER_HUB_TAGS_URL: &str = "https://hub.docker.com/v2/repositories/";
const DOCKER_HUB_TAG_PAGE_SIZE: u32 = 20;
const DOCKER_HUB_TAG_FETCH_CONCURRENCY: usize = 8;
const DOCKER_HUB_LIBRARY_PAGE_SIZE: u32 = 100;
const DOCKER_PULL: &str = "command -v docker >/dev/null 2>&1 || exit 90; docker pull \"$1\"";
const DOCKER_PRUNE: &str = "command -v docker >/dev/null 2>&1 || exit 90; docker image prune -f";
const DOCKER_RMI: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
failed=0
for id in "$@"; do
  docker rmi "$id" || failed=1
done
exit "$failed"
"#;
const DOCKER_RUN: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
name="$1"
image="$2"
p1="$3"; p2="$4"; p3="$5"; p4="$6"; p5="$7"; p6="$8"; p7="$9"; p8="$10"
set -- docker run -d --name "$name"
[ -n "$p1" ] && set -- "$@" -p "$p1"
[ -n "$p2" ] && set -- "$@" -p "$p2"
[ -n "$p3" ] && set -- "$@" -p "$p3"
[ -n "$p4" ] && set -- "$@" -p "$p4"
[ -n "$p5" ] && set -- "$@" -p "$p5"
[ -n "$p6" ] && set -- "$@" -p "$p6"
[ -n "$p7" ] && set -- "$@" -p "$p7"
[ -n "$p8" ] && set -- "$@" -p "$p8"
set -- "$@" "$image"
"$@"
"#;
const DOCKER_NETWORK_CREATE: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
name="$1"
driver="$2"
subnet="$3"
gateway="$4"
l1="$5"; l2="$6"; l3="$7"; l4="$8"
set -- docker network create --driver "$driver"
[ -n "$subnet" ] && set -- "$@" --subnet "$subnet"
[ -n "$gateway" ] && set -- "$@" --gateway "$gateway"
[ -n "$l1" ] && set -- "$@" --label "$l1"
[ -n "$l2" ] && set -- "$@" --label "$l2"
[ -n "$l3" ] && set -- "$@" --label "$l3"
[ -n "$l4" ] && set -- "$@" --label "$l4"
set -- "$@" "$name"
"$@"
"#;
const DOCKER_NETWORK_RM: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
failed=0
for id in "$@"; do
  docker network rm "$id" || failed=1
done
exit "$failed"
"#;
const DOCKER_NETWORK_PRUNE: &str =
    "command -v docker >/dev/null 2>&1 || exit 90; docker network prune -f";
/// Inspect all volumes as JSON lines. Empty host has no names — exit cleanly.
const DOCKER_VOLUMES: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
names=$(docker volume ls -q)
[ -z "$names" ] && exit 0
# Intentional unquoted expansion: one volume name per word for inspect.
docker volume inspect $names --format '{{json .}}'
"#;
/// Map container names to mounted volume names (one tab-separated pair per line).
const DOCKER_VOLUME_USERS: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
ids=$(docker ps -aq)
[ -z "$ids" ] && exit 0
# Intentional unquoted expansion: one container id per word for inspect.
docker inspect $ids --format '{{range .Mounts}}{{if eq .Type "volume"}}{{printf "%s\t%s\n" $.Name .Name}}{{end}}{{end}}'
"#;
const DOCKER_VOLUME_CREATE: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
name="$1"
driver="$2"
l1="$3"; l2="$4"; l3="$5"; l4="$6"
set -- docker volume create
[ -n "$driver" ] && set -- "$@" --driver "$driver"
[ -n "$l1" ] && set -- "$@" --label "$l1"
[ -n "$l2" ] && set -- "$@" --label "$l2"
[ -n "$l3" ] && set -- "$@" --label "$l3"
[ -n "$l4" ] && set -- "$@" --label "$l4"
set -- "$@" "$name"
"$@"
"#;
const DOCKER_VOLUME_RM: &str = r#"command -v docker >/dev/null 2>&1 || exit 90
failed=0
for name in "$@"; do
  docker volume rm "$name" || failed=1
done
exit "$failed"
"#;
const DOCKER_VOLUME_PRUNE: &str =
    "command -v docker >/dev/null 2>&1 || exit 90; docker volume prune -f";
const PM2_LIST: &str = "command -v pm2 >/dev/null 2>&1 || exit 90; pm2 jlist";
const NGINX_TEST: &str = "command -v nginx >/dev/null 2>&1 || exit 90; nginx -t";
const NGINX_RELOAD: &str = "command -v nginx >/dev/null 2>&1 || exit 90; nginx -s reload";
const MAX_IMAGE_REMOVE: usize = 50;
const MAX_NETWORK_REMOVE: usize = 50;
const MAX_NETWORK_LABELS: usize = 4;
const MAX_VOLUME_REMOVE: usize = 50;
const MAX_VOLUME_LABELS: usize = 4;
const MAX_PUBLISH_PORTS: usize = 8;
const BUILTIN_NETWORKS: &[&str] = &["bridge", "host", "none"];
const MAX_DOCKER_SEARCH_LIMIT: u32 = 100;
const MAX_DOCKER_OFFICIAL_LIST: u32 = 300;
const DEFAULT_DOCKER_SEARCH_LIMIT: u32 = 25;
const MAX_DOCKER_SEARCH_QUERY_LEN: usize = 128;

pub struct OperationsService;

impl OperationsService {
    pub async fn docker_list(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<DockerContainer>> {
        let result = sessions
            .exec(session_id, RemoteCommand::script(DOCKER_LIST, Vec::new()))
            .await?;
        ensure_supported(&result)?;
        let mut containers = result
            .stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(2_000)
            .map(parse_docker_container)
            .collect::<AppResult<Vec<_>>>()?;

        if let Ok(stats) = sessions
            .exec(session_id, RemoteCommand::script(DOCKER_STATS, Vec::new()))
            .await
        {
            if stats.exit_code == 0 {
                merge_docker_stats(&mut containers, &stats.stdout);
            }
        }

        Ok(containers)
    }

    pub async fn docker_images_list(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<DockerImage>> {
        let images_result = sessions
            .exec(session_id, RemoteCommand::script(DOCKER_IMAGES, Vec::new()))
            .await?;
        ensure_supported(&images_result)?;
        let mut images = images_result
            .stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(2_000)
            .map(parse_docker_image)
            .collect::<AppResult<Vec<_>>>()?;

        if let Ok(containers_result) = sessions
            .exec(session_id, RemoteCommand::script(DOCKER_LIST, Vec::new()))
            .await
        {
            if containers_result.exit_code == 0 {
                let containers = containers_result
                    .stdout
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .take(2_000)
                    .filter_map(|line| parse_docker_container(line).ok())
                    .collect::<Vec<_>>();
                attach_image_users(&mut images, &containers);
            }
        }

        Ok(images)
    }

    pub async fn docker_images_search(
        query: String,
        limit: u32,
    ) -> AppResult<Vec<DockerOnlineImage>> {
        let query = query.trim();
        if query.is_empty() {
            // Empty query lists official library images for the Online images default view.
            return list_docker_hub_official(clamp_official_limit(limit)).await;
        }
        validate_search_query(query)?;
        let limit = clamp_search_limit(limit);
        search_docker_hub(query, limit).await
    }

    pub async fn docker_action(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        container: String,
        action: ResourceAction,
    ) -> AppResult<OperationResult> {
        validate_target(&container)?;
        let verb = action_verb(action);
        let script = "command -v docker >/dev/null 2>&1 || exit 90; docker \"$1\" \"$2\"";
        run_operation(
            sessions,
            session_id,
            RemoteCommand::script(script, vec![verb.into(), container]),
        )
        .await
    }

    pub async fn docker_image_action(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        action: DockerImageAction,
    ) -> AppResult<OperationResult> {
        match action {
            DockerImageAction::Pull { reference } => {
                validate_image_reference(&reference)?;
                run_operation(
                    sessions,
                    session_id,
                    RemoteCommand::script(DOCKER_PULL, vec![reference])
                        .with_timeout(std::time::Duration::from_secs(300)),
                )
                .await
            }
            DockerImageAction::Remove { ids } => {
                if ids.is_empty() || ids.len() > MAX_IMAGE_REMOVE {
                    return Err(AppError::InvalidOperation);
                }
                for id in &ids {
                    validate_image_reference(id)?;
                }
                run_operation(sessions, session_id, RemoteCommand::script(DOCKER_RMI, ids)).await
            }
            DockerImageAction::Prune => {
                run_operation(
                    sessions,
                    session_id,
                    RemoteCommand::script(DOCKER_PRUNE, Vec::new()),
                )
                .await
            }
            DockerImageAction::CreateContainer {
                image,
                name,
                publish_ports,
            } => {
                validate_image_reference(&image)?;
                validate_container_name(&name)?;
                if publish_ports.len() > MAX_PUBLISH_PORTS {
                    return Err(AppError::InvalidOperation);
                }
                for port in &publish_ports {
                    validate_publish_port(port)?;
                }
                let mut args = vec![name, image];
                args.extend(publish_ports);
                while args.len() < 2 + MAX_PUBLISH_PORTS {
                    args.push(String::new());
                }
                run_operation(
                    sessions,
                    session_id,
                    RemoteCommand::script(DOCKER_RUN, args),
                )
                .await
            }
        }
    }

    pub async fn docker_networks_list(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<DockerNetwork>> {
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(DOCKER_NETWORKS, Vec::new()),
            )
            .await?;
        ensure_supported(&result)?;
        result
            .stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(2_000)
            .map(parse_docker_network)
            .collect()
    }

    pub async fn docker_network_action(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        action: DockerNetworkAction,
    ) -> AppResult<OperationResult> {
        match action {
            DockerNetworkAction::Create {
                name,
                driver,
                subnet,
                gateway,
                labels,
            } => {
                validate_network_name(&name)?;
                validate_network_driver(&driver)?;
                let subnet = subnet.trim().to_owned();
                let gateway = gateway.trim().to_owned();
                if !subnet.is_empty() {
                    validate_ipv4_cidr(&subnet)?;
                }
                if !gateway.is_empty() {
                    validate_ipv4_address(&gateway)?;
                }
                if labels.len() > MAX_NETWORK_LABELS {
                    return Err(AppError::InvalidOperation);
                }
                for label in &labels {
                    validate_network_label(label)?;
                }
                let mut args = vec![name, driver, subnet, gateway];
                args.extend(labels);
                while args.len() < 4 + MAX_NETWORK_LABELS {
                    args.push(String::new());
                }
                run_operation(
                    sessions,
                    session_id,
                    RemoteCommand::script(DOCKER_NETWORK_CREATE, args),
                )
                .await
            }
            DockerNetworkAction::Remove { ids } => {
                if ids.is_empty() || ids.len() > MAX_NETWORK_REMOVE {
                    return Err(AppError::InvalidOperation);
                }
                for id in &ids {
                    validate_network_remove_target(id)?;
                }
                run_operation(
                    sessions,
                    session_id,
                    RemoteCommand::script(DOCKER_NETWORK_RM, ids),
                )
                .await
            }
            DockerNetworkAction::Prune => {
                run_operation(
                    sessions,
                    session_id,
                    RemoteCommand::script(DOCKER_NETWORK_PRUNE, Vec::new()),
                )
                .await
            }
        }
    }

    pub async fn docker_volumes_list(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<DockerVolume>> {
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(DOCKER_VOLUMES, Vec::new()),
            )
            .await?;
        ensure_supported(&result)?;
        let mut volumes = result
            .stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(2_000)
            .map(parse_docker_volume)
            .collect::<AppResult<Vec<_>>>()?;

        if let Ok(users_result) = sessions
            .exec(
                session_id,
                RemoteCommand::script(DOCKER_VOLUME_USERS, Vec::new()),
            )
            .await
        {
            if users_result.exit_code == 0 {
                attach_volume_users(&mut volumes, &users_result.stdout);
            }
        }

        Ok(volumes)
    }

    pub async fn docker_volume_action(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        action: DockerVolumeAction,
    ) -> AppResult<OperationResult> {
        match action {
            DockerVolumeAction::Create {
                name,
                driver,
                labels,
            } => {
                validate_volume_name(&name)?;
                let driver = driver.trim().to_owned();
                if !driver.is_empty() {
                    validate_volume_driver(&driver)?;
                }
                if labels.len() > MAX_VOLUME_LABELS {
                    return Err(AppError::InvalidOperation);
                }
                for label in &labels {
                    validate_network_label(label)?;
                }
                let mut args = vec![name, driver];
                args.extend(labels);
                while args.len() < 2 + MAX_VOLUME_LABELS {
                    args.push(String::new());
                }
                run_operation(
                    sessions,
                    session_id,
                    RemoteCommand::script(DOCKER_VOLUME_CREATE, args),
                )
                .await
            }
            DockerVolumeAction::Remove { names } => {
                if names.is_empty() || names.len() > MAX_VOLUME_REMOVE {
                    return Err(AppError::InvalidOperation);
                }
                for name in &names {
                    validate_volume_remove_target(name)?;
                }
                run_operation(
                    sessions,
                    session_id,
                    RemoteCommand::script(DOCKER_VOLUME_RM, names),
                )
                .await
            }
            DockerVolumeAction::Prune => {
                run_operation(
                    sessions,
                    session_id,
                    RemoteCommand::script(DOCKER_VOLUME_PRUNE, Vec::new()),
                )
                .await
            }
        }
    }

    pub async fn pm2_list(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<Pm2Process>> {
        let result = sessions
            .exec(session_id, RemoteCommand::script(PM2_LIST, Vec::new()))
            .await?;
        ensure_supported(&result)?;
        parse_pm2(&result.stdout)
    }

    pub async fn pm2_action(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        process: String,
        action: ResourceAction,
    ) -> AppResult<OperationResult> {
        validate_target(&process)?;
        let script = "command -v pm2 >/dev/null 2>&1 || exit 90; pm2 \"$1\" \"$2\"";
        run_operation(
            sessions,
            session_id,
            RemoteCommand::script(script, vec![action_verb(action).into(), process]),
        )
        .await
    }

    pub async fn nginx_action(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        action: NginxAction,
    ) -> AppResult<OperationResult> {
        let script = match action {
            NginxAction::Test => NGINX_TEST,
            NginxAction::Reload => NGINX_RELOAD,
        };
        run_operation(
            sessions,
            session_id,
            RemoteCommand::script(script, Vec::new()),
        )
        .await
    }

    pub async fn logs(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        source: LogSource,
        target: Option<String>,
        lines: u32,
    ) -> AppResult<OperationResult> {
        if !(20..=5_000).contains(&lines) {
            return Err(AppError::InvalidOperation);
        }
        let lines = lines.to_string();
        let (script, args) = match source {
            LogSource::System => (
                "if command -v journalctl >/dev/null 2>&1; then journalctl -n \"$1\" --no-pager; elif test -r /var/log/syslog; then tail -n \"$1\" /var/log/syslog; else exit 90; fi",
                vec![lines],
            ),
            LogSource::Auth => (
                "if test -r /var/log/auth.log; then tail -n \"$1\" /var/log/auth.log; elif test -r /var/log/secure; then tail -n \"$1\" /var/log/secure; else exit 90; fi",
                vec![lines],
            ),
            LogSource::NginxAccess => (
                "test -r /var/log/nginx/access.log || exit 90; tail -n \"$1\" /var/log/nginx/access.log",
                vec![lines],
            ),
            LogSource::NginxError => (
                "test -r /var/log/nginx/error.log || exit 90; tail -n \"$1\" /var/log/nginx/error.log",
                vec![lines],
            ),
            LogSource::Docker => {
                let target = required_target(target)?;
                ("command -v docker >/dev/null 2>&1 || exit 90; docker logs --tail \"$1\" \"$2\"", vec![lines, target])
            }
            LogSource::Pm2 => {
                let target = required_target(target)?;
                ("command -v pm2 >/dev/null 2>&1 || exit 90; pm2 logs \"$2\" --lines \"$1\" --nostream", vec![lines, target])
            }
            LogSource::Service => {
                let target = required_target(target)?;
                ("command -v journalctl >/dev/null 2>&1 || exit 90; journalctl -u \"$2\" -n \"$1\" --no-pager", vec![lines, target])
            }
        };
        run_operation(
            sessions,
            session_id,
            RemoteCommand::script(script, args).with_output_limit(1024 * 1024),
        )
        .await
    }
}

async fn run_operation(
    sessions: &ServerSessionManager,
    session_id: SessionId,
    command: RemoteCommand,
) -> AppResult<OperationResult> {
    let result = sessions.exec(session_id, command).await?;
    if result.exit_code == 90 {
        return Err(AppError::UnsupportedRemote);
    }
    Ok(OperationResult {
        success: result.exit_code == 0,
        output: combined_output(result),
    })
}

fn ensure_supported(result: &RemoteExecResult) -> AppResult<()> {
    match result.exit_code {
        0 => Ok(()),
        90 => Err(AppError::UnsupportedRemote),
        _ => Err(AppError::ExecFailed),
    }
}

fn combined_output(result: RemoteExecResult) -> String {
    let stdout = result.stdout.trim();
    let stderr = result.stderr.trim();
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n{stderr}"),
        (false, true) => stdout.to_owned(),
        (true, false) => stderr.to_owned(),
        (true, true) => String::new(),
    }
}

fn action_verb(action: ResourceAction) -> &'static str {
    match action {
        ResourceAction::Start => "start",
        ResourceAction::Stop => "stop",
        ResourceAction::Restart => "restart",
    }
}

fn required_target(target: Option<String>) -> AppResult<String> {
    let target = target.ok_or(AppError::InvalidOperation)?;
    validate_target(&target)?;
    Ok(target)
}

fn validate_target(target: &str) -> AppResult<()> {
    if target.is_empty()
        || target.len() > 256
        || !target.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'@' | b':' | b'/')
        })
    {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn validate_image_reference(reference: &str) -> AppResult<()> {
    validate_target(reference)
}

fn validate_search_query(query: &str) -> AppResult<()> {
    let query = query.trim();
    if query.is_empty()
        || query.len() > MAX_DOCKER_SEARCH_QUERY_LEN
        || !query.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':')
        })
    {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn clamp_search_limit(limit: u32) -> u32 {
    if limit == 0 {
        DEFAULT_DOCKER_SEARCH_LIMIT
    } else {
        limit.min(MAX_DOCKER_SEARCH_LIMIT)
    }
}

fn clamp_official_limit(limit: u32) -> u32 {
    if limit == 0 {
        MAX_DOCKER_OFFICIAL_LIST
    } else {
        limit.min(MAX_DOCKER_OFFICIAL_LIST)
    }
}

fn validate_container_name(name: &str) -> AppResult<()> {
    if name.is_empty()
        || name.len() > 128
        || name.starts_with('-')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn validate_network_name(name: &str) -> AppResult<()> {
    validate_container_name(name)?;
    if is_builtin_network(name) {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn validate_network_driver(driver: &str) -> AppResult<()> {
    match driver {
        "bridge" | "overlay" | "macvlan" | "ipvlan" => Ok(()),
        _ => Err(AppError::InvalidOperation),
    }
}

fn validate_network_remove_target(target: &str) -> AppResult<()> {
    validate_target(target)?;
    if is_builtin_network(target) {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn validate_volume_name(name: &str) -> AppResult<()> {
    validate_container_name(name)
}

fn validate_volume_driver(driver: &str) -> AppResult<()> {
    if driver.is_empty()
        || driver.len() > 64
        || !driver
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn validate_volume_remove_target(name: &str) -> AppResult<()> {
    validate_target(name)
}

fn is_builtin_network(name: &str) -> bool {
    BUILTIN_NETWORKS.iter().any(|item| *item == name)
}

fn validate_ipv4_address(value: &str) -> AppResult<()> {
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 4 {
        return Err(AppError::InvalidOperation);
    }
    for part in parts {
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(AppError::InvalidOperation);
        }
        let octet: u8 = part.parse().map_err(|_| AppError::InvalidOperation)?;
        if part.len() > 1 && part.starts_with('0') {
            return Err(AppError::InvalidOperation);
        }
        let _ = octet;
    }
    Ok(())
}

fn validate_ipv4_cidr(value: &str) -> AppResult<()> {
    let (address, prefix) = value.split_once('/').ok_or(AppError::InvalidOperation)?;
    validate_ipv4_address(address)?;
    if prefix.is_empty() || prefix.len() > 2 || !prefix.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(AppError::InvalidOperation);
    }
    let bits: u8 = prefix.parse().map_err(|_| AppError::InvalidOperation)?;
    if bits > 32 {
        return Err(AppError::InvalidOperation);
    }
    Ok(())
}

fn validate_network_label(label: &str) -> AppResult<()> {
    let (key, value) = label.split_once('=').ok_or(AppError::InvalidOperation)?;
    if key.is_empty()
        || key.len() > 128
        || value.len() > 256
        || !key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'_' | b'-' | b'.' | b':' | b'/' | b'@' | b'+')
        })
    {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn validate_publish_port(port: &str) -> AppResult<()> {
    let (host, container) = port.split_once(':').ok_or(AppError::InvalidOperation)?;
    if host.is_empty()
        || container.is_empty()
        || host.contains(':')
        || container.contains(':')
        || !is_valid_port_number(host)
        || !is_valid_port_number(container)
    {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn is_valid_port_number(value: &str) -> bool {
    if value.is_empty() || value.len() > 5 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    matches!(value.parse::<u16>(), Ok(port) if port > 0)
}

fn parse_docker_container(line: &str) -> AppResult<DockerContainer> {
    let value: Value = serde_json::from_str(line).map_err(|_| AppError::ExecFailed)?;
    Ok(DockerContainer {
        id: json_string(&value, "ID")?,
        name: json_string(&value, "Names")?,
        image: json_string(&value, "Image")?,
        state: json_string(&value, "State")?,
        status: json_string(&value, "Status")?,
        ports: optional_json_string(&value, "Ports").unwrap_or_default(),
        cpu_percent: 0.0,
        memory_bytes: 0,
    })
}

fn parse_docker_image(line: &str) -> AppResult<DockerImage> {
    let value: Value = serde_json::from_str(line).map_err(|_| AppError::ExecFailed)?;
    let id = json_string(&value, "ID")?;
    let repository = optional_json_string(&value, "Repository").unwrap_or_else(|| "<none>".into());
    let tag = optional_json_string(&value, "Tag").unwrap_or_else(|| "<none>".into());
    let name = format_image_name(&repository, &tag);
    let size_bytes = optional_json_string(&value, "Size")
        .and_then(|size| parse_docker_size(&size))
        .unwrap_or(0);
    let created_at_epoch_seconds = optional_json_string(&value, "CreatedAt")
        .and_then(|created| parse_docker_created_at(&created))
        .unwrap_or(0);
    Ok(DockerImage {
        id,
        name,
        size_bytes,
        created_at_epoch_seconds,
        used_by: Vec::new(),
    })
}

fn parse_docker_network(line: &str) -> AppResult<DockerNetwork> {
    let value: Value = serde_json::from_str(line).map_err(|_| AppError::ExecFailed)?;
    let id = json_string(&value, "Id").or_else(|_| json_string(&value, "ID"))?;
    let name = json_string(&value, "Name")?;
    let driver = optional_json_string(&value, "Driver").unwrap_or_default();
    let (ipv4_subnet, ipv4_gateway) = parse_network_ipam(&value);
    let labels = format_network_labels(value.get("Labels"));
    let created_at_epoch_seconds = optional_json_string(&value, "Created")
        .and_then(|created| parse_network_created_at(&created))
        .unwrap_or(0);
    Ok(DockerNetwork {
        id,
        name,
        driver,
        ipv4_subnet,
        ipv4_gateway,
        labels,
        created_at_epoch_seconds,
    })
}

fn parse_docker_volume(line: &str) -> AppResult<DockerVolume> {
    let value: Value = serde_json::from_str(line).map_err(|_| AppError::ExecFailed)?;
    let name = json_string(&value, "Name")?;
    let driver = optional_json_string(&value, "Driver").unwrap_or_default();
    let mountpoint = optional_json_string(&value, "Mountpoint").unwrap_or_default();
    let scope = optional_json_string(&value, "Scope").unwrap_or_default();
    let labels = format_network_labels(value.get("Labels"));
    let created_at_epoch_seconds = optional_json_string(&value, "CreatedAt")
        .and_then(|created| parse_network_created_at(&created))
        .unwrap_or(0);
    Ok(DockerVolume {
        name,
        driver,
        mountpoint,
        scope,
        labels,
        created_at_epoch_seconds,
        used_by: Vec::new(),
    })
}

fn attach_volume_users(volumes: &mut [DockerVolume], output: &str) {
    for line in output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(10_000)
    {
        let Some((container_name, volume_name)) = line.split_once('\t') else {
            continue;
        };
        let container_name = normalize_container_name(container_name);
        let volume_name = volume_name.trim();
        if container_name.is_empty() || volume_name.is_empty() {
            continue;
        }
        if let Some(volume) = volumes.iter_mut().find(|volume| volume.name == volume_name) {
            if !volume
                .used_by
                .iter()
                .any(|existing| existing == &container_name)
            {
                volume.used_by.push(container_name);
            }
        }
    }
}

fn parse_network_ipam(value: &Value) -> (String, String) {
    let Some(configs) = value
        .get("IPAM")
        .and_then(|ipam| ipam.get("Config"))
        .and_then(Value::as_array)
    else {
        return (String::new(), String::new());
    };
    for config in configs {
        let subnet = optional_json_string(config, "Subnet").unwrap_or_default();
        if subnet.is_empty() || !subnet.contains('.') {
            continue;
        }
        let gateway = optional_json_string(config, "Gateway").unwrap_or_default();
        return (subnet, gateway);
    }
    (String::new(), String::new())
}

fn format_network_labels(labels: Option<&Value>) -> String {
    let Some(Value::Object(map)) = labels else {
        return String::new();
    };
    let mut pairs = map
        .iter()
        .filter_map(|(key, value)| {
            let value = value.as_str()?;
            Some(format!("{key}:{value}"))
        })
        .collect::<Vec<_>>();
    pairs.sort();
    pairs.join(", ")
}

/// Parses Docker network `Created` like `2024-08-09T18:32:12.123456789Z`.
fn parse_network_created_at(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let (datetime, offset) = if let Some(stripped) = value.strip_suffix('Z') {
        (stripped.to_owned(), "+0000".to_owned())
    } else if value.len() >= 6 {
        let split = value.len() - 6;
        let tz = &value[split..];
        let sign = tz.as_bytes().first().copied()?;
        if (sign == b'+' || sign == b'-') && tz.as_bytes().get(3) == Some(&b':') {
            let offset = format!("{}{}{}", sign as char, &tz[1..3], &tz[4..6]);
            (value[..split].to_owned(), offset)
        } else {
            return parse_docker_created_at(value);
        }
    } else {
        return parse_docker_created_at(value);
    };
    let datetime = datetime
        .split('.')
        .next()
        .unwrap_or(datetime.as_str())
        .to_owned();
    let (date, time) = datetime.split_once('T')?;
    parse_docker_created_at(&format!("{date} {time} {offset}"))
}

fn parse_hub_online_image(value: &Value) -> AppResult<DockerOnlineImage> {
    let name = json_string(value, "repo_name")?;
    if name.is_empty() || name.len() > 256 {
        return Err(AppError::ExecFailed);
    }
    let description = optional_json_string(value, "short_description").unwrap_or_default();
    Ok(DockerOnlineImage {
        name,
        description,
        star_count: json_u64(value, "star_count").unwrap_or(0),
        is_official: json_bool(value, "is_official").unwrap_or(false),
        is_automated: json_bool(value, "is_automated").unwrap_or(false),
        tags: Vec::new(),
    })
}

fn parse_hub_library_image(value: &Value) -> AppResult<DockerOnlineImage> {
    let name = json_string(value, "name")?;
    if name.is_empty() || name.len() > 256 || name.contains('/') {
        return Err(AppError::ExecFailed);
    }
    let description = optional_json_string(value, "description")
        .or_else(|| optional_json_string(value, "short_description"))
        .unwrap_or_default();
    Ok(DockerOnlineImage {
        name,
        description,
        star_count: json_u64(value, "star_count").unwrap_or(0),
        is_official: true,
        is_automated: json_bool(value, "is_automated").unwrap_or(false),
        tags: Vec::new(),
    })
}

#[derive(Debug, Deserialize)]
struct HubSearchResponse {
    results: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct HubLibraryResponse {
    results: Vec<Value>,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HubTagsResponse {
    results: Vec<Value>,
}

fn hub_http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("Runory/0.1 (Docker Hub catalog)")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(45))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|_| AppError::ExecFailed)
}

fn map_hub_request_error(error: reqwest::Error) -> AppError {
    if error.is_timeout() || error.is_connect() {
        AppError::ConnectionTimeout
    } else {
        AppError::ExecFailed
    }
}

fn hub_repository_path(name: &str) -> String {
    if name.contains('/') {
        name.to_owned()
    } else {
        format!("library/{name}")
    }
}

fn hub_tags_url(name: &str) -> AppResult<reqwest::Url> {
    let path = hub_repository_path(name);
    let base = format!("{DOCKER_HUB_TAGS_URL}{path}/tags/");
    let mut url = reqwest::Url::parse(&base).map_err(|_| AppError::InvalidOperation)?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("page_size", &DOCKER_HUB_TAG_PAGE_SIZE.to_string());
        pairs.append_pair("ordering", "last_updated");
    }
    Ok(url)
}

fn parse_hub_tags_body(body: &str) -> Vec<String> {
    let Ok(parsed) = serde_json::from_str::<HubTagsResponse>(body) else {
        return Vec::new();
    };
    let mut tags = Vec::new();
    for value in parsed.results {
        let Some(tag) = optional_json_string(&value, "name") else {
            continue;
        };
        if tag.is_empty() || tag.len() > 128 {
            continue;
        }
        if !tags.iter().any(|existing| existing == &tag) {
            tags.push(tag);
        }
    }
    prefer_latest_tag(&mut tags);
    tags
}

fn prefer_latest_tag(tags: &mut Vec<String>) {
    if let Some(index) = tags.iter().position(|tag| tag == "latest") {
        if index > 0 {
            let latest = tags.remove(index);
            tags.insert(0, latest);
        }
    }
}

async fn fetch_hub_tags(client: &reqwest::Client, name: &str) -> Vec<String> {
    let Ok(url) = hub_tags_url(name) else {
        return Vec::new();
    };
    let Ok(response) = client.get(url).send().await else {
        return Vec::new();
    };
    if !response.status().is_success() {
        return Vec::new();
    }
    let Ok(body) = response.text().await else {
        return Vec::new();
    };
    parse_hub_tags_body(&body)
}

async fn enrich_hub_images_with_tags(
    client: &reqwest::Client,
    mut images: Vec<DockerOnlineImage>,
) -> Vec<DockerOnlineImage> {
    let names: Vec<String> = images.iter().map(|image| image.name.clone()).collect();
    let tag_lists = stream::iter(names)
        .map(|name| async move {
            let tags = fetch_hub_tags(client, &name).await;
            (name, tags)
        })
        .buffer_unordered(DOCKER_HUB_TAG_FETCH_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    for (name, tags) in tag_lists {
        if let Some(image) = images.iter_mut().find(|image| image.name == name) {
            image.tags = tags;
        }
    }
    images
}

async fn list_docker_hub_official(limit: u32) -> AppResult<Vec<DockerOnlineImage>> {
    match list_docker_hub_official_remote(limit).await {
        Ok(images) if !images.is_empty() => Ok(images),
        Ok(_) | Err(_) => {
            // Keep the Online images default view usable when Hub is unreachable.
            let mut images = fallback_official_images();
            images.truncate(limit as usize);
            Ok(images)
        }
    }
}

async fn list_docker_hub_official_remote(limit: u32) -> AppResult<Vec<DockerOnlineImage>> {
    let client = hub_http_client()?;
    let mut images = Vec::new();
    let mut page = 1u32;
    while (images.len() as u32) < limit && page <= 20 {
        let page_size = (limit - images.len() as u32).min(DOCKER_HUB_LIBRARY_PAGE_SIZE);
        let mut url =
            reqwest::Url::parse(DOCKER_HUB_LIBRARY_URL).map_err(|_| AppError::InvalidOperation)?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("page", &page.to_string());
            pairs.append_pair("page_size", &page_size.to_string());
        }
        let response = client
            .get(url)
            .send()
            .await
            .map_err(map_hub_request_error)?;
        if !response.status().is_success() {
            return Err(AppError::ExecFailed);
        }
        let body = response.text().await.map_err(|_| AppError::ExecFailed)?;
        let parsed: HubLibraryResponse =
            serde_json::from_str(&body).map_err(|_| AppError::ExecFailed)?;
        let batch = parsed
            .results
            .into_iter()
            .filter_map(|value| parse_hub_library_image(&value).ok())
            .collect::<Vec<_>>();
        if batch.is_empty() {
            break;
        }
        let fetched = batch.len();
        images.extend(batch);
        let has_next = parsed
            .next
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        if !has_next || fetched < page_size as usize {
            break;
        }
        page += 1;
    }
    images.truncate(limit as usize);
    // Official catalog is large; skip tag enrichment and let the UI fall back to `latest`.
    Ok(images)
}

fn fallback_official_images() -> Vec<DockerOnlineImage> {
    const FALLBACK: &[(&str, &str)] = &[
        ("nginx", "Official build of Nginx."),
        ("ubuntu", "Ubuntu is a Debian-based Linux operating system."),
        ("alpine", "A minimal Docker image based on Alpine Linux."),
        ("redis", "Redis is an open source key-value store."),
        ("postgres", "The PostgreSQL object-relational database system."),
        ("mysql", "MySQL is a widely used open-source relational database."),
        ("node", "Node.js is a JavaScript runtime built on Chrome's V8 JavaScript engine."),
        ("python", "Python is an interpreted, interactive, object-oriented, open-source programming language."),
        ("mongo", "MongoDB document-oriented database."),
        ("httpd", "The Apache HTTP Server Project."),
        ("busybox", "Busybox base image."),
        ("debian", "Debian is a Linux distribution made of free and open-source software."),
        ("golang", "Go (golang) is a general purpose, higher-level, imperative programming language."),
        ("mariadb", "MariaDB Server is a high performance open source database server."),
        ("memcached", "Free & open source, high-performance, distributed memory object caching system."),
        ("rabbitmq", "RabbitMQ is an open source multi-protocol messaging broker."),
        ("wordpress", "The WordPress blogging software."),
        ("traefik", "Traefik is a modern HTTP reverse proxy and load balancer."),
        ("hello-world", "Hello World! (an example of minimal Dockerization)"),
        ("eclipse-temurin", "Official images for Eclipse Temurin builds of OpenJDK."),
    ];
    FALLBACK
        .iter()
        .map(|(name, description)| DockerOnlineImage {
            name: (*name).to_owned(),
            description: (*description).to_owned(),
            star_count: 0,
            is_official: true,
            is_automated: false,
            tags: vec!["latest".to_owned()],
        })
        .collect()
}

async fn search_docker_hub(query: &str, limit: u32) -> AppResult<Vec<DockerOnlineImage>> {
    let mut url =
        reqwest::Url::parse(DOCKER_HUB_SEARCH_URL).map_err(|_| AppError::InvalidOperation)?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("query", query);
        pairs.append_pair("page_size", &limit.to_string());
    }
    let client = hub_http_client()?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(map_hub_request_error)?;
    if !response.status().is_success() {
        return Err(AppError::ExecFailed);
    }
    let body = response.text().await.map_err(|_| AppError::ExecFailed)?;
    let images = parse_hub_search_body(&body, limit)?;
    let tag_client = reqwest::Client::builder()
        .user_agent("Runory/0.1 (Docker Hub catalog)")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|_| AppError::ExecFailed)?;
    Ok(enrich_hub_images_with_tags(&tag_client, images).await)
}

fn parse_hub_search_body(body: &str, limit: u32) -> AppResult<Vec<DockerOnlineImage>> {
    let parsed: HubSearchResponse = serde_json::from_str(body).map_err(|_| AppError::ExecFailed)?;
    Ok(parsed
        .results
        .into_iter()
        .filter_map(|value| parse_hub_online_image(&value).ok())
        .take(limit as usize)
        .collect())
}

fn format_image_name(repository: &str, tag: &str) -> String {
    if repository == "<none>" && tag == "<none>" {
        "<none>".into()
    } else if tag == "<none>" {
        repository.to_owned()
    } else {
        format!("{repository}:{tag}")
    }
}

fn attach_image_users(images: &mut [DockerImage], containers: &[DockerContainer]) {
    for container in containers {
        let container_name = normalize_container_name(&container.name);
        let names = container_name
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if names.is_empty() {
            continue;
        }
        for image in images.iter_mut() {
            if image_matches_container(image, &container.image) {
                for name in &names {
                    if !image.used_by.iter().any(|existing| existing == name) {
                        image.used_by.push(name.clone());
                    }
                }
            }
        }
    }
}

fn image_matches_container(image: &DockerImage, container_image: &str) -> bool {
    let container_image = container_image.trim();
    if container_image.is_empty() {
        return false;
    }
    if image.name != "<none>" && image.name == container_image {
        return true;
    }
    let image_id = strip_sha256_prefix(&image.id);
    let container_id = strip_sha256_prefix(container_image);
    !image_id.is_empty()
        && !container_id.is_empty()
        && (image_id.starts_with(container_id) || container_id.starts_with(image_id))
}

fn strip_sha256_prefix(value: &str) -> &str {
    value.strip_prefix("sha256:").unwrap_or(value)
}

fn merge_docker_stats(containers: &mut [DockerContainer], output: &str) {
    for line in output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(2_000)
    {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(stats_id) = value
            .get("ID")
            .and_then(Value::as_str)
            .or_else(|| value.get("Container").and_then(Value::as_str))
        else {
            continue;
        };
        let stats_name = value
            .get("Name")
            .and_then(Value::as_str)
            .map(normalize_container_name);
        let cpu_percent = value
            .get("CPUPerc")
            .and_then(Value::as_str)
            .and_then(parse_cpu_percent)
            .unwrap_or(0.0);
        let memory_bytes = value
            .get("MemUsage")
            .and_then(Value::as_str)
            .and_then(parse_mem_usage_bytes)
            .unwrap_or(0);

        if let Some(container) = containers.iter_mut().find(|container| {
            container_id_matches(&container.id, stats_id)
                || stats_name
                    .as_ref()
                    .is_some_and(|name| container_name_matches(&container.name, name))
        }) {
            container.cpu_percent = cpu_percent;
            container.memory_bytes = memory_bytes;
        }
    }
}

fn container_id_matches(container_id: &str, stats_id: &str) -> bool {
    !stats_id.is_empty()
        && (container_id.starts_with(stats_id) || stats_id.starts_with(container_id))
}

fn container_name_matches(container_name: &str, stats_name: &str) -> bool {
    normalize_container_name(container_name)
        .split(',')
        .any(|part| part == stats_name)
}

fn normalize_container_name(name: &str) -> String {
    name.trim().trim_start_matches('/').to_owned()
}

fn parse_cpu_percent(value: &str) -> Option<f64> {
    value.trim().trim_end_matches('%').trim().parse().ok()
}

fn parse_mem_usage_bytes(value: &str) -> Option<u64> {
    let used = value.split('/').next()?.trim();
    parse_docker_size(used)
}

fn parse_docker_size(value: &str) -> Option<u64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let split = value
        .find(|ch: char| ch.is_ascii_alphabetic())
        .unwrap_or(value.len());
    let (number, unit) = value.split_at(split);
    let amount: f64 = number.trim().parse().ok()?;
    if !amount.is_finite() || amount < 0.0 {
        return None;
    }
    let multiplier = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "kb" | "kib" => 1024.0,
        "mb" | "mib" => 1024.0 * 1024.0,
        "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "tb" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((amount * multiplier).round() as u64)
}

/// Parses Docker `CreatedAt` like `2026-03-25 06:12:14 +0000 UTC`.
fn parse_docker_created_at(value: &str) -> Option<u64> {
    let mut parts = value.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    let offset = parts.next().unwrap_or("+0000");
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: u32 = date_parts.next()?.parse().ok()?;
    let day: u32 = date_parts.next()?.parse().ok()?;
    let mut time_parts = time.split(':');
    let hour: u32 = time_parts.next()?.parse().ok()?;
    let minute: u32 = time_parts.next()?.parse().ok()?;
    let second: u32 = time_parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    let mut epoch = days
        .checked_mul(86_400)?
        .checked_add(i64::from(hour) * 3_600)?
        .checked_add(i64::from(minute) * 60)?
        .checked_add(i64::from(second))?;
    epoch = apply_timezone_offset(epoch, offset)?;
    u64::try_from(epoch).ok()
}

fn apply_timezone_offset(epoch: i64, offset: &str) -> Option<i64> {
    if offset.len() != 5 {
        return Some(epoch);
    }
    let sign = match offset.as_bytes()[0] {
        b'+' => 1_i64,
        b'-' => -1,
        _ => return Some(epoch),
    };
    let hours: i64 = offset[1..3].parse().ok()?;
    let minutes: i64 = offset[3..5].parse().ok()?;
    if hours > 14 || minutes > 59 {
        return None;
    }
    epoch.checked_sub(sign * (hours * 3_600 + minutes * 60))
}

fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    let mut y = year;
    let m = i64::from(month);
    let d = i64::from(day);
    y -= i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn optional_json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| value.len() <= 4096)
        .map(ToOwned::to_owned)
}

fn parse_pm2(output: &str) -> AppResult<Vec<Pm2Process>> {
    let values: Vec<Value> = serde_json::from_str(output).map_err(|_| AppError::ExecFailed)?;
    values
        .into_iter()
        .take(2_000)
        .map(|value| {
            let environment = value.get("pm2_env").ok_or(AppError::ExecFailed)?;
            let monitoring = value.get("monit").ok_or(AppError::ExecFailed)?;
            Ok(Pm2Process {
                id: value
                    .get("pm_id")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or(AppError::ExecFailed)?,
                name: json_string(&value, "name")?,
                status: json_string(environment, "status")?,
                cpu_percent: monitoring.get("cpu").and_then(Value::as_f64).unwrap_or(0.0),
                memory_bytes: monitoring
                    .get("memory")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            })
        })
        .collect()
}

fn json_string(value: &Value, key: &str) -> AppResult<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| value.len() <= 4096)
        .map(ToOwned::to_owned)
        .ok_or(AppError::ExecFailed)
}

fn json_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|item| match item {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        Value::Bool(flag) => Some(u64::from(*flag)),
        _ => None,
    })
}

fn json_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(|item| match item {
        Value::Bool(flag) => Some(*flag),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "[ok]" => Some(true),
            "false" | "0" | "no" | "" => Some(false),
            _ => None,
        },
        Value::Number(number) => number.as_u64().map(|n| n != 0),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_docker_json_without_shell_columns() {
        let item = parse_docker_container(
            r#"{"ID":"abc","Names":"web","Image":"nginx:latest","State":"running","Status":"Up","Ports":"0.0.0.0:8080->80/tcp"}"#,
        )
        .expect("container");
        assert_eq!(item.name, "web");
        assert_eq!(item.ports, "0.0.0.0:8080->80/tcp");
        assert_eq!(item.cpu_percent, 0.0);
        assert_eq!(item.memory_bytes, 0);
    }

    #[test]
    fn merges_docker_stats_by_name_and_id_prefix() {
        let mut containers = vec![
            parse_docker_container(
                r#"{"ID":"abcdef123456","Names":"web","Image":"nginx","State":"running","Status":"Up","Ports":"80/tcp"}"#,
            )
            .expect("container"),
            parse_docker_container(
                r#"{"ID":"deadbeef0001","Names":"db","Image":"postgres","State":"exited","Status":"Exited","Ports":""}"#,
            )
            .expect("stopped"),
        ];
        merge_docker_stats(
            &mut containers,
            r#"{"ID":"abcdef123456","Name":"web","CPUPerc":"12.50%","MemUsage":"1.5MiB / 7.7GiB"}
{"ID":"deadbeef","Name":"db","CPUPerc":"0.00%","MemUsage":"0B / 0B"}"#,
        );
        assert_eq!(containers[0].cpu_percent, 12.5);
        assert_eq!(containers[0].memory_bytes, 1_572_864);
        assert_eq!(containers[1].cpu_percent, 0.0);
        assert_eq!(containers[1].memory_bytes, 0);
    }

    #[test]
    fn parses_docker_size_units() {
        assert_eq!(parse_docker_size("746.5kB"), Some(764_416));
        assert_eq!(parse_docker_size("1.5MiB"), Some(1_572_864));
        assert_eq!(parse_docker_size("2GiB"), Some(2_147_483_648));
        assert_eq!(parse_docker_size("0B"), Some(0));
    }

    #[test]
    fn parses_docker_image_json_and_created_at() {
        let item = parse_docker_image(
            r#"{"ID":"sha256:0cf1d6af5ca7abcdef","Repository":"docker.m.daocloud.io/nginx","Tag":"latest","Size":"153.46MB","CreatedAt":"2026-03-25 06:12:14 +0000 UTC"}"#,
        )
        .expect("image");
        assert_eq!(item.id, "sha256:0cf1d6af5ca7abcdef");
        assert_eq!(item.name, "docker.m.daocloud.io/nginx:latest");
        assert_eq!(item.size_bytes, 160_914_473);
        assert_eq!(item.created_at_epoch_seconds, 1_774_419_134);
        assert!(item.used_by.is_empty());
    }

    #[test]
    fn formats_dangling_image_name() {
        let item = parse_docker_image(
            r#"{"ID":"sha256:abc","Repository":"<none>","Tag":"<none>","Size":"1MB","CreatedAt":"2026-01-01 00:00:00 +0000 UTC"}"#,
        )
        .expect("dangling");
        assert_eq!(item.name, "<none>");
    }

    #[test]
    fn attaches_containers_using_image_by_name_and_id() {
        let mut images = vec![
            parse_docker_image(
                r#"{"ID":"sha256:0cf1d6af5ca7abcdef","Repository":"nginx","Tag":"latest","Size":"10MB","CreatedAt":"2026-03-25 06:12:14 +0000 UTC"}"#,
            )
            .expect("nginx"),
            parse_docker_image(
                r#"{"ID":"sha256:deadbeef0001abcd","Repository":"redis","Tag":"7","Size":"20MB","CreatedAt":"2026-03-25 06:12:14 +0000 UTC"}"#,
            )
            .expect("redis"),
        ];
        let containers = vec![
            parse_docker_container(
                r#"{"ID":"c1","Names":"nginx","Image":"nginx:latest","State":"running","Status":"Up","Ports":""}"#,
            )
            .expect("by name"),
            parse_docker_container(
                r#"{"ID":"c2","Names":"/redis603","Image":"sha256:deadbeef0001","State":"running","Status":"Up","Ports":""}"#,
            )
            .expect("by id"),
        ];
        attach_image_users(&mut images, &containers);
        assert_eq!(images[0].used_by, vec!["nginx".to_owned()]);
        assert_eq!(images[1].used_by, vec!["redis603".to_owned()]);
    }

    #[test]
    fn rejects_invalid_image_action_inputs() {
        assert!(validate_image_reference("nginx:latest").is_ok());
        assert!(matches!(
            validate_image_reference("nginx; rm -rf /"),
            Err(AppError::InvalidOperation)
        ));
        assert!(validate_container_name("web-1").is_ok());
        assert!(matches!(
            validate_container_name("-web"),
            Err(AppError::InvalidOperation)
        ));
        assert!(validate_publish_port("8080:80").is_ok());
        assert!(matches!(
            validate_publish_port("80"),
            Err(AppError::InvalidOperation)
        ));
        assert!(matches!(
            validate_publish_port("0:80"),
            Err(AppError::InvalidOperation)
        ));
        assert!(matches!(
            validate_publish_port("8080:abc"),
            Err(AppError::InvalidOperation)
        ));
    }

    #[test]
    fn operation_targets_reject_metacharacters() {
        assert!(validate_target("web-1").is_ok());
        assert!(matches!(
            validate_target("web; reboot"),
            Err(AppError::InvalidOperation)
        ));
    }

    #[test]
    fn parses_docker_hub_search_payload() {
        let body = r#"{
            "count": 2,
            "results": [
                {
                    "repo_name": "nginx",
                    "short_description": "Official build of Nginx.",
                    "star_count": 19712,
                    "is_official": true,
                    "is_automated": false
                },
                {
                    "repo_name": "bitnami/redis",
                    "short_description": "",
                    "star_count": "120",
                    "is_official": false,
                    "is_automated": "false"
                }
            ]
        }"#;
        let items = parse_hub_search_body(body, 25).expect("hub search");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "nginx");
        assert_eq!(items[0].description, "Official build of Nginx.");
        assert_eq!(items[0].star_count, 19712);
        assert!(items[0].is_official);
        assert!(!items[0].is_automated);
        assert_eq!(items[1].name, "bitnami/redis");
        assert_eq!(items[1].star_count, 120);
        assert!(!items[1].is_official);
        assert!(items[0].tags.is_empty());
    }

    #[test]
    fn parses_hub_tags_and_prefers_latest() {
        let body = r#"{
            "results": [
                {"name": "1.25"},
                {"name": "latest"},
                {"name": "1.25-alpine"},
                {"name": ""}
            ]
        }"#;
        let tags = parse_hub_tags_body(body);
        assert_eq!(tags, vec!["latest", "1.25", "1.25-alpine"]);
        assert_eq!(hub_repository_path("nginx"), "library/nginx");
        assert_eq!(hub_repository_path("bitnami/redis"), "bitnami/redis");
    }

    #[test]
    fn hub_search_respects_limit_and_rejects_bad_payload() {
        let body =
            r#"{"results":[{"repo_name":"a","star_count":1},{"repo_name":"b","star_count":2}]}"#;
        let items = parse_hub_search_body(body, 1).expect("limited");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "a");
        assert!(matches!(
            parse_hub_search_body("not-json", 10),
            Err(AppError::ExecFailed)
        ));
        let skipped =
            parse_hub_search_body(r#"{"results":[{"repo_name":""},{"repo_name":"ok"}]}"#, 10)
                .expect("skip empty");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].name, "ok");
    }

    #[test]
    fn fallback_official_images_are_marked_official() {
        let items = fallback_official_images();
        assert!(items.len() >= 10);
        assert!(items.iter().all(|item| item.is_official));
        assert!(items.iter().any(|item| item.name == "nginx"));
        assert_eq!(items[0].tags, vec!["latest"]);
    }

    #[test]
    fn parses_hub_library_official_payload() {
        let body = r#"{
            "count": 2,
            "next": "https://hub.docker.com/v2/repositories/library/?page=2",
            "results": [
                {
                    "name": "ubuntu",
                    "namespace": "library",
                    "description": "Ubuntu is a Debian-based Linux operating system.",
                    "star_count": 17871
                },
                {
                    "name": "nginx",
                    "description": "Official build of Nginx.",
                    "star_count": 19712,
                    "is_automated": false
                }
            ]
        }"#;
        let parsed: HubLibraryResponse = serde_json::from_str(body).expect("library json");
        let items = parsed
            .results
            .into_iter()
            .map(|value| parse_hub_library_image(&value).expect("image"))
            .collect::<Vec<_>>();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "ubuntu");
        assert!(items[0].is_official);
        assert_eq!(items[1].name, "nginx");
        assert_eq!(items[1].star_count, 19712);
        assert_eq!(clamp_official_limit(0), MAX_DOCKER_OFFICIAL_LIST);
        assert_eq!(clamp_official_limit(50), 50);
        assert_eq!(clamp_official_limit(999), MAX_DOCKER_OFFICIAL_LIST);
    }

    #[test]
    fn validates_search_query_and_clamps_limit() {
        assert!(validate_search_query("nginx").is_ok());
        assert!(validate_search_query("bitnami/redis").is_ok());
        assert!(matches!(
            validate_search_query(""),
            Err(AppError::InvalidOperation)
        ));
        assert!(matches!(
            validate_search_query("nginx; rm"),
            Err(AppError::InvalidOperation)
        ));
        assert_eq!(clamp_search_limit(0), DEFAULT_DOCKER_SEARCH_LIMIT);
        assert_eq!(clamp_search_limit(25), 25);
        assert_eq!(clamp_search_limit(500), MAX_DOCKER_SEARCH_LIMIT);
    }

    #[test]
    fn parses_docker_network_inspect_json() {
        let item = parse_docker_network(
            r#"{
                "Name":"docker_default",
                "Id":"abcdef0123456789",
                "Created":"2024-08-09T18:32:12.123456789Z",
                "Driver":"bridge",
                "IPAM":{"Config":[{"Subnet":"172.25.0.0/16","Gateway":"172.25.0.1"}]},
                "Labels":{"com.docker.compose.network":"default","com.docker.compose.project":"docker"}
            }"#,
        )
        .expect("network");
        assert_eq!(item.name, "docker_default");
        assert_eq!(item.id, "abcdef0123456789");
        assert_eq!(item.driver, "bridge");
        assert_eq!(item.ipv4_subnet, "172.25.0.0/16");
        assert_eq!(item.ipv4_gateway, "172.25.0.1");
        assert!(item.labels.contains("com.docker.compose.network:default"));
        assert_eq!(item.created_at_epoch_seconds, 1_723_228_332);
    }

    #[test]
    fn parses_builtin_network_without_ipam() {
        let item = parse_docker_network(
            r#"{"Name":"none","Id":"abc","Created":"2024-08-09T18:32:12Z","Driver":"null","IPAM":{"Config":null},"Labels":{}}"#,
        )
        .expect("none");
        assert_eq!(item.name, "none");
        assert_eq!(item.driver, "null");
        assert!(item.ipv4_subnet.is_empty());
        assert!(item.ipv4_gateway.is_empty());
        assert!(item.labels.is_empty());
    }

    #[test]
    fn rejects_invalid_network_action_inputs() {
        assert!(validate_network_name("app-net").is_ok());
        assert!(matches!(
            validate_network_name("bridge"),
            Err(AppError::InvalidOperation)
        ));
        assert!(matches!(
            validate_network_name("net; rm"),
            Err(AppError::InvalidOperation)
        ));
        assert!(validate_network_driver("bridge").is_ok());
        assert!(matches!(
            validate_network_driver("custom"),
            Err(AppError::InvalidOperation)
        ));
        assert!(validate_ipv4_cidr("172.17.0.0/16").is_ok());
        assert!(matches!(
            validate_ipv4_cidr("172.17.0.0"),
            Err(AppError::InvalidOperation)
        ));
        assert!(matches!(
            validate_ipv4_cidr("172.17.0.0/99"),
            Err(AppError::InvalidOperation)
        ));
        assert!(validate_ipv4_address("172.17.0.1").is_ok());
        assert!(matches!(
            validate_ipv4_address("172.17.0"),
            Err(AppError::InvalidOperation)
        ));
        assert!(validate_network_label("com.example.key=value").is_ok());
        assert!(matches!(
            validate_network_label("nocolon"),
            Err(AppError::InvalidOperation)
        ));
        assert!(matches!(
            validate_network_remove_target("bridge"),
            Err(AppError::InvalidOperation)
        ));
        assert!(matches!(
            validate_network_remove_target("host"),
            Err(AppError::InvalidOperation)
        ));
        assert!(matches!(
            validate_network_remove_target("none"),
            Err(AppError::InvalidOperation)
        ));
        assert!(validate_network_remove_target("abcdef012345").is_ok());
    }

    #[test]
    fn parses_docker_volume_inspect_json() {
        let item = parse_docker_volume(
            r#"{
                "CreatedAt":"2024-08-09T18:32:12.123456789Z",
                "Driver":"local",
                "Labels":{"com.docker.volume.anonymous":""},
                "Mountpoint":"/var/lib/docker/volumes/b47dde889551a55d/_data",
                "Name":"b47dde889551a55d",
                "Options":null,
                "Scope":"local"
            }"#,
        )
        .expect("volume");
        assert_eq!(item.name, "b47dde889551a55d");
        assert_eq!(item.driver, "local");
        assert_eq!(
            item.mountpoint,
            "/var/lib/docker/volumes/b47dde889551a55d/_data"
        );
        assert_eq!(item.scope, "local");
        assert_eq!(item.labels, "com.docker.volume.anonymous:");
        assert_eq!(item.created_at_epoch_seconds, 1_723_228_332);
        assert!(item.used_by.is_empty());
    }

    #[test]
    fn attaches_volume_users_from_inspect_lines() {
        let mut volumes = vec![
            parse_docker_volume(
                r#"{"Name":"data","Driver":"local","Mountpoint":"/var/lib/docker/volumes/data/_data","Scope":"local","Labels":{},"CreatedAt":"2024-08-09T18:32:12Z"}"#,
            )
            .expect("data"),
            parse_docker_volume(
                r#"{"Name":"cache","Driver":"local","Mountpoint":"/var/lib/docker/volumes/cache/_data","Scope":"local","Labels":{},"CreatedAt":"2024-08-09T18:32:12Z"}"#,
            )
            .expect("cache"),
        ];
        attach_volume_users(&mut volumes, "/redis\tdata\n/nginx\tdata\n/redis\tcache\n");
        assert_eq!(
            volumes[0].used_by,
            vec!["redis".to_owned(), "nginx".to_owned()]
        );
        assert_eq!(volumes[1].used_by, vec!["redis".to_owned()]);
    }

    #[test]
    fn rejects_invalid_volume_action_inputs() {
        assert!(validate_volume_name("app-data").is_ok());
        assert!(matches!(
            validate_volume_name("vol; rm"),
            Err(AppError::InvalidOperation)
        ));
        assert!(validate_volume_driver("local").is_ok());
        assert!(matches!(
            validate_volume_driver("local;evil"),
            Err(AppError::InvalidOperation)
        ));
        assert!(validate_volume_remove_target("b47dde889551a55d").is_ok());
        assert!(matches!(
            validate_volume_remove_target("vol; rm"),
            Err(AppError::InvalidOperation)
        ));
    }
}
