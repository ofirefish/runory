use std::collections::HashSet;

use crate::domain::{AppError, AppResult, CronEntry, CronSchedule, CronTask, EnvironmentEntry};

pub(crate) fn serialize_environment(entries: Vec<EnvironmentEntry>) -> AppResult<Vec<u8>> {
    if entries.is_empty() || entries.len() > 256 {
        return Err(AppError::InvalidOperation);
    }
    let mut output = String::new();
    let mut keys = HashSet::new();
    for entry in entries {
        if entry.key.is_empty()
            || entry.key.len() > 128
            || !entry.key.bytes().enumerate().all(|(index, byte)| {
                byte.is_ascii_uppercase() || byte.is_ascii_digit() && index > 0 || byte == b'_'
            })
            || !keys.insert(entry.key.clone())
            || entry.value.len() > 64 * 1024
            || entry.value.contains(['\0', '\n', '\r'])
        {
            return Err(AppError::InvalidOperation);
        }
        let escaped = entry
            .value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`");
        output.push_str(&entry.key);
        output.push_str("=\"");
        output.push_str(&escaped);
        output.push_str("\"\n");
    }
    if output.len() > 1024 * 1024 {
        Err(AppError::InvalidOperation)
    } else {
        Ok(output.into_bytes())
    }
}

pub(crate) fn cron_kind(task: &CronTask) -> &'static str {
    match task {
        CronTask::Backup { .. } => "backup",
        CronTask::ServiceRestart { .. } => "service-restart",
        CronTask::GitPull { .. } => "git-pull",
    }
}

fn cron_schedule(schedule: CronSchedule) -> &'static str {
    match schedule {
        CronSchedule::Hourly => "0 * * * *",
        CronSchedule::Daily => "0 2 * * *",
        CronSchedule::Weekly => "0 3 * * 0",
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(crate) fn render_cron(entry: &CronEntry, task: &CronTask) -> AppResult<String> {
    let command = match task {
        CronTask::Backup {
            source_path,
            destination_directory,
        } => format!(
            "mkdir -p {} && tar -czf {}/runory-cron-$(date -u +\\%Y\\%m\\%dT\\%H\\%M\\%SZ).tar.gz -- {}",
            quote(destination_directory),
            quote(destination_directory),
            quote(source_path)
        ),
        CronTask::ServiceRestart { service } => {
            format!("systemctl restart {}", quote(service))
        }
        CronTask::GitPull {
            repository_path,
            branch,
        } => format!(
            "git -C {} pull --ff-only origin {}",
            quote(repository_path),
            quote(branch)
        ),
    };
    Ok(format!(
        "{} {} # runory:{}:{}:{}\n",
        cron_schedule(entry.schedule),
        command,
        entry.id,
        match entry.schedule {
            CronSchedule::Hourly => "hourly",
            CronSchedule::Daily => "daily",
            CronSchedule::Weekly => "weekly",
        },
        entry.task_kind
    ))
}

pub(crate) fn parse_cron(output: &str) -> AppResult<Vec<CronEntry>> {
    output
        .lines()
        .take(500)
        .filter_map(|line| line.split_once("# runory:").map(|(_, marker)| marker))
        .map(|marker| {
            let mut parts = marker.trim().split(':');
            let id = parts
                .next()
                .ok_or(AppError::ExecFailed)?
                .parse()
                .map_err(|_| AppError::ExecFailed)?;
            let schedule = match parts.next() {
                Some("hourly") => CronSchedule::Hourly,
                Some("daily") => CronSchedule::Daily,
                Some("weekly") => CronSchedule::Weekly,
                _ => return Err(AppError::ExecFailed),
            };
            let task_kind = parts
                .next()
                .filter(|value| value.len() <= 64)
                .ok_or(AppError::ExecFailed)?
                .to_owned();
            Ok(CronEntry {
                id,
                schedule,
                task_kind,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn environment_rejects_duplicate_and_multiline_values() {
        assert!(serialize_environment(vec![EnvironmentEntry {
            key: "TOKEN".into(),
            value: "one\ntwo".into(),
        }])
        .is_err());
    }

    #[test]
    fn cron_templates_roundtrip_the_runory_marker() {
        let entry = CronEntry {
            id: Uuid::nil(),
            schedule: CronSchedule::Daily,
            task_kind: "service-restart".into(),
        };
        let line = render_cron(
            &entry,
            &CronTask::ServiceRestart {
                service: "nginx.service".into(),
            },
        )
        .expect("render cron");
        let parsed = parse_cron(&line).expect("parse cron");
        assert_eq!(parsed.first().map(|value| value.id), Some(Uuid::nil()));
    }
}
