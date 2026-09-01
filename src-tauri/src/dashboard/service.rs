use crate::domain::{
    AppError, AppResult, DiskUsage, ProcessInfo, ServerDashboard, ServiceHealth, ServiceStatus,
    SessionId,
};
use crate::ssh::{RemoteCommand, RemoteExecResult, ServerSessionManager};

const METRICS_SCRIPT: &str = r#"
test -r /proc/stat -a -r /proc/meminfo -a -r /proc/uptime || exit 90
read tag u n s i w q z st g gn < /proc/stat
t1=$((u+n+s+i+w+q+z+st)); i1=$((i+w)); sleep 0.25
read tag u n s i w q z st g gn < /proc/stat
t2=$((u+n+s+i+w+q+z+st)); i2=$((i+w)); dt=$((t2-t1)); di=$((i2-i1))
if test "$dt" -gt 0; then cpu=$(((dt-di)*100/dt)); else cpu=0; fi
printf 'CPU\t%s\n' "$cpu"
awk '/MemTotal:/ {t=$2} /MemAvailable:/ {a=$2} END {printf "MEM\t%.0f\t%.0f\n", (t-a)*1024, t*1024}' /proc/meminfo
awk '{printf "UP\t%.0f\n", $1}' /proc/uptime
awk -F'[: ]+' 'NR>2 {rx+=$3; tx+=$11} END {printf "NET\t%.0f\t%.0f\n", rx, tx}' /proc/net/dev
df -Pk | awk 'NR>1 {gsub(/%/,"",$5); printf "DISK\t%s\t%.0f\t%.0f\t%s\n", $6, $3*1024, $2*1024, $5}'
"#;

const PROCESS_SCRIPT: &str =
    "ps -eo pid=,user=,pcpu=,pmem=,comm= --sort=-pcpu 2>/dev/null | head -n 50";
const SERVICES_SCRIPT: &str = r#"
command -v systemctl >/dev/null 2>&1 || exit 90
shift
for service in "$@"; do
  status=$(systemctl is-active "$service" 2>/dev/null || true)
  printf '%s\t%s\n' "$service" "$status"
done
"#;

pub struct DashboardService;

impl DashboardService {
    pub async fn overview(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<ServerDashboard> {
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(METRICS_SCRIPT, Vec::new()),
            )
            .await?;
        ensure_supported(&result)?;
        parse_dashboard(&result.stdout)
    }

    pub async fn processes(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<ProcessInfo>> {
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(PROCESS_SCRIPT, Vec::new()).with_output_limit(512 * 1024),
            )
            .await?;
        ensure_success(&result)?;
        parse_processes(&result.stdout)
    }

    pub async fn service_health(
        sessions: &ServerSessionManager,
        session_id: SessionId,
        services: Vec<String>,
    ) -> AppResult<Vec<ServiceHealth>> {
        if services.is_empty() || services.len() > 32 {
            return Err(AppError::InvalidOperation);
        }
        for service in &services {
            validate_identifier(service)?;
        }
        let result = sessions
            .exec(session_id, RemoteCommand::script(SERVICES_SCRIPT, services))
            .await?;
        ensure_supported(&result)?;
        parse_services(&result.stdout)
    }
}

fn ensure_success(result: &RemoteExecResult) -> AppResult<()> {
    if result.exit_code == 0 {
        Ok(())
    } else {
        Err(AppError::ExecFailed)
    }
}

fn ensure_supported(result: &RemoteExecResult) -> AppResult<()> {
    if result.exit_code == 90 {
        Err(AppError::UnsupportedRemote)
    } else {
        ensure_success(result)
    }
}

fn parse_dashboard(output: &str) -> AppResult<ServerDashboard> {
    let mut dashboard = ServerDashboard {
        cpu_usage_percent: 0.0,
        memory_used_bytes: 0,
        memory_total_bytes: 0,
        uptime_seconds: 0,
        network_received_bytes: 0,
        network_transmitted_bytes: 0,
        disks: Vec::new(),
    };
    let mut seen = [false; 4];
    for line in output.lines().take(512) {
        let fields = line.split('\t').collect::<Vec<_>>();
        match fields.as_slice() {
            ["CPU", value] => {
                dashboard.cpu_usage_percent = parse_f64(value)?;
                seen[0] = true;
            }
            ["MEM", used, total] => {
                dashboard.memory_used_bytes = parse_u64(used)?;
                dashboard.memory_total_bytes = parse_u64(total)?;
                seen[1] = true;
            }
            ["UP", value] => {
                dashboard.uptime_seconds = parse_u64(value)?;
                seen[2] = true;
            }
            ["NET", received, transmitted] => {
                dashboard.network_received_bytes = parse_u64(received)?;
                dashboard.network_transmitted_bytes = parse_u64(transmitted)?;
                seen[3] = true;
            }
            ["DISK", mount, used, total, percent] if mount.len() <= 4096 => {
                dashboard.disks.push(DiskUsage {
                    mount: (*mount).to_owned(),
                    used_bytes: parse_u64(used)?,
                    total_bytes: parse_u64(total)?,
                    usage_percent: parse_f64(percent)?,
                });
            }
            _ => {}
        }
    }
    if seen.into_iter().all(|value| value) {
        Ok(dashboard)
    } else {
        Err(AppError::ExecFailed)
    }
}

fn parse_processes(output: &str) -> AppResult<Vec<ProcessInfo>> {
    output
        .lines()
        .take(50)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split_whitespace();
            Ok(ProcessInfo {
                pid: fields
                    .next()
                    .ok_or(AppError::ExecFailed)?
                    .parse()
                    .map_err(|_| AppError::ExecFailed)?,
                user: bounded(fields.next())?,
                cpu_percent: parse_f64(fields.next().ok_or(AppError::ExecFailed)?)?,
                memory_percent: parse_f64(fields.next().ok_or(AppError::ExecFailed)?)?,
                command: bounded(fields.next())?,
            })
        })
        .collect()
}

fn parse_services(output: &str) -> AppResult<Vec<ServiceHealth>> {
    output
        .lines()
        .take(32)
        .map(|line| {
            let (name, raw) = line.split_once('\t').ok_or(AppError::ExecFailed)?;
            Ok(ServiceHealth {
                name: bounded(Some(name))?,
                status: match raw.trim() {
                    "active" => ServiceStatus::Active,
                    "inactive" => ServiceStatus::Inactive,
                    "failed" => ServiceStatus::Failed,
                    _ => ServiceStatus::Unknown,
                },
            })
        })
        .collect()
}

pub(crate) fn validate_identifier(value: &str) -> AppResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'@' | b':')
        })
    {
        Err(AppError::InvalidOperation)
    } else {
        Ok(())
    }
}

fn bounded(value: Option<&str>) -> AppResult<String> {
    value
        .filter(|value| !value.is_empty() && value.len() <= 4096)
        .map(ToOwned::to_owned)
        .ok_or(AppError::ExecFailed)
}

fn parse_u64(value: &str) -> AppResult<u64> {
    value.trim().parse().map_err(|_| AppError::ExecFailed)
}

fn parse_f64(value: &str) -> AppResult<f64> {
    let value = value
        .trim()
        .parse::<f64>()
        .map_err(|_| AppError::ExecFailed)?;
    if value.is_finite() {
        Ok(value.clamp(0.0, 100.0))
    } else {
        Err(AppError::ExecFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_linux_metrics() {
        let value = parse_dashboard(
            "CPU\t12.5\nMEM\t512\t1024\nUP\t99\nNET\t1000\t2000\nDISK\t/\t50\t100\t50\n",
        )
        .expect("dashboard");
        assert_eq!(value.memory_total_bytes, 1024);
        assert_eq!(value.disks.len(), 1);
    }

    #[test]
    fn identifiers_reject_shell_metacharacters() {
        assert!(validate_identifier("nginx.service").is_ok());
        assert!(matches!(
            validate_identifier("nginx; reboot"),
            Err(AppError::InvalidOperation)
        ));
    }
}
