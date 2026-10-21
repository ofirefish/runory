use std::time::Duration;

use crate::domain::{
    AppError, AppResult, OperationResult, PhpProjectConfig, PhpRuntime, PythonServiceAction,
    SessionId,
};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::php_render::render_php_files;
use super::php_validation::{
    php_nginx_conf_name, php_service_name, validate_php_bin, validate_php_config, validate_php_path,
};
use super::service::{ensure_operation, join_outputs, DeploymentService};
use super::validation::validate_app_name;

/// Exit 90 = missing PHP/php-fpm/nginx/systemd/composer, 91 = invalid account or path.
/// Prints `FPM <path>` for the php-fpm binary whose version matches the chosen CLI.
const PREPARE_SCRIPT: &str = r#"set -e
PATH="$PATH:/usr/sbin:/usr/local/sbin:/sbin"
command -v "$2" >/dev/null 2>&1 || exit 90
command -v nginx >/dev/null 2>&1 || exit 90
command -v systemctl >/dev/null 2>&1 || exit 90
test -d /run/systemd/system || exit 90
want=$("$2" -r 'echo PHP_MAJOR_VERSION . "." . PHP_MINOR_VERSION;')
suffix="${2#php}"
fpm=""
for candidate in "php-fpm$suffix" "php-fpm$(printf '%s' "$suffix" | tr -d .)" php-fpm; do
  path=$(command -v "$candidate" 2>/dev/null) || continue
  have=$("$path" -v 2>/dev/null | sed -n '1s/^PHP \([0-9][0-9]*\.[0-9][0-9]*\).*/\1/p')
  if test "$have" = "$want"; then fpm="$path"; break; fi
done
test -n "$fpm" || exit 90
id "$4" >/dev/null 2>&1 || exit 91
test -d "$1" || exit 91
if test -n "$3"; then test -d "$1/$3" || exit 91; fi
if test "$5" = 1 && test -f "$1/composer.json"; then
  command -v composer >/dev/null 2>&1 || exit 90
  cd "$1"
  COMPOSER_ALLOW_SUPERUSER=1 composer install --no-dev --no-interaction --prefer-dist
fi
printf 'FPM %s\n' "$fpm"
"#;

/// Installs the unit and nginx site. A site that fails `nginx -t` is rolled back so an
/// invalid project can never take down the other sites served by the same nginx.
const INSTALL_SCRIPT: &str = r#"set -e
PATH="$PATH:/usr/sbin:/usr/local/sbin:/sbin"
dir="$1/.runory"
mkdir -p "$dir"
awk -v dir="$dir" '/^#--runory-file:/{out=dir "/" substr($0,16); next} out!=""{print > out}'
if test "$(id -u)" = 0; then
  SUDO=""
elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  SUDO="sudo -n"
else
  printf 'UNIT_PENDING %s\n' "$dir"
  exit 1
fi
nginx_user=$(awk '/^[[:space:]]*user[[:space:]]/{gsub(/;/,"",$2); print $2; exit}' /etc/nginx/nginx.conf 2>/dev/null || true)
case "$nginx_user" in ''|*[!A-Za-z0-9_-]*) nginx_user=www-data ;; esac
id "$nginx_user" >/dev/null 2>&1 || nginx_user=nginx
sed -i "s/@NGINX_USER@/$nginx_user/g" "$dir/fpm.conf"
sed -i "s#@FPM_BIN@#$5#g" "$dir/unit"
unit="/etc/systemd/system/$2"
site="/etc/nginx/conf.d/$3"
had_site=0
if test -f "$site"; then had_site=1; $SUDO cp "$site" "$dir/nginx.conf.bak"; fi
$SUDO cp "$dir/unit" "$unit"
$SUDO cp "$dir/nginx.conf" "$site"
$SUDO systemctl daemon-reload
if test "$4" = 1; then $SUDO systemctl enable "$2"; else $SUDO systemctl disable "$2"; fi
$SUDO systemctl restart "$2"
sleep 1
systemctl is-active --quiet "$2"
if ! $SUDO nginx -t 2>"$dir/nginx-test.log"; then
  if test "$had_site" = 1; then $SUDO cp "$dir/nginx.conf.bak" "$site"; else $SUDO rm -f "$site"; fi
  cat "$dir/nginx-test.log" >&2
  exit 1
fi
if systemctl is-active --quiet nginx; then $SUDO systemctl reload nginx; else $SUDO systemctl start nginx; fi
printf 'SERVICE %s\nSITE %s\n' "$2" "$3"
"#;

impl DeploymentService {
    pub async fn php_control(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: &str,
        action: PythonServiceAction,
    ) -> AppResult<OperationResult> {
        let service = php_service_name(name)?;
        let operation = match action {
            PythonServiceAction::Start => "php-start",
            PythonServiceAction::Stop => "php-stop",
            PythonServiceAction::Restart => "php-restart",
        };
        self.systemd_control(sessions, session_id, service, action, operation)
            .await
    }

    pub async fn php_provision(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: String,
        project_path: String,
        php: PhpProjectConfig,
    ) -> AppResult<OperationResult> {
        validate_app_name(&name)?;
        validate_php_path(&project_path)?;
        validate_php_config(&php)?;
        let service = php_service_name(&name)?;
        let nginx_conf = php_nginx_conf_name(&name)?;
        let files = render_php_files(&name, &project_path, &php)?;
        let profile_id = sessions.profile_id(session_id).await?;

        let result = async {
            let prepare = ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(
                            PREPARE_SCRIPT,
                            vec![
                                project_path.clone(),
                                php.php_bin.clone(),
                                php.run_dir.clone(),
                                php.run_user.clone(),
                                if php.composer_install { "1" } else { "0" }.into(),
                            ],
                        )
                        .with_timeout(Duration::from_secs(900)),
                    )
                    .await?,
            )?;
            if !prepare.success {
                return Ok(prepare);
            }
            let fpm_bin = parse_fpm_binary(&prepare.output).ok_or(AppError::UnsupportedRemote)?;

            let install = ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(
                            INSTALL_SCRIPT,
                            vec![
                                project_path.clone(),
                                service.clone(),
                                nginx_conf,
                                if php.auto_start { "1" } else { "0" }.into(),
                                fpm_bin,
                            ],
                        )
                        .with_stdin(files.bundle().into_bytes())?
                        .with_timeout(Duration::from_secs(120)),
                    )
                    .await?,
            )?;
            Ok(join_outputs(vec![prepare, install]))
        }
        .await;

        self.finish(profile_id, "php-provision", project_path, result)
            .await
    }

    pub async fn list_php_runtimes(
        sessions: &ServerSessionManager,
        session_id: SessionId,
    ) -> AppResult<Vec<PhpRuntime>> {
        let result = sessions
            .exec(
                session_id,
                RemoteCommand::script(
                    r#"found=0
for bin in php php8.5 php8.4 php8.3 php8.2 php8.1 php8.0 php7.4 php7.3 php7.2; do
  if command -v "$bin" >/dev/null 2>&1; then
    ver=$("$bin" -r 'echo PHP_VERSION;' 2>/dev/null || true)
    printf '%s\t%s\n' "$bin" "$ver"
    found=1
  fi
done
if test "$found" = "0"; then exit 90; fi
"#,
                    Vec::new(),
                )
                .with_timeout(Duration::from_secs(30)),
            )
            .await?;
        if result.exit_code == 90 {
            return Ok(Vec::new());
        }
        if result.exit_code != 0 {
            return Err(AppError::ExecFailed);
        }
        Ok(parse_php_runtimes(&result.stdout))
    }
}

fn parse_fpm_binary(output: &str) -> Option<String> {
    let path = output
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix("FPM "))?
        .trim();
    // The path is spliced into a sed replacement and a unit file; keep it strictly absolute and plain.
    validate_php_path(path).ok()?;
    Some(path.to_string())
}

fn parse_php_runtimes(output: &str) -> Vec<PhpRuntime> {
    let mut runtimes: Vec<PhpRuntime> = Vec::new();
    for line in output.lines() {
        let mut parts = line.split('\t');
        let binary = parts.next().unwrap_or("").trim();
        let version = parts.next().unwrap_or("").trim();
        if validate_php_bin(binary).is_err() || runtimes.iter().any(|item| item.binary == binary) {
            continue;
        }
        runtimes.push(PhpRuntime {
            binary: binary.to_string(),
            version: version.chars().take(32).collect(),
        });
        if runtimes.len() >= 16 {
            break;
        }
    }
    runtimes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_php_runtime_lines_and_skips_unknown_binaries() {
        let runtimes = parse_php_runtimes(
            "php\t8.3.6\nphp8.2\t8.2.20\nphp\t8.3.6\npython3\t3.12\nphp;id\t1\n",
        );
        assert_eq!(runtimes.len(), 2);
        assert_eq!(runtimes[0].binary, "php");
        assert_eq!(runtimes[1].version, "8.2.20");
    }

    #[test]
    fn fpm_binary_must_be_a_plain_absolute_path() {
        assert_eq!(
            parse_fpm_binary("composer output\nFPM /usr/sbin/php-fpm8.2\n").as_deref(),
            Some("/usr/sbin/php-fpm8.2")
        );
        assert!(parse_fpm_binary("FPM php-fpm").is_none());
        assert!(parse_fpm_binary("FPM /usr/sbin/php-fpm;id").is_none());
        assert!(parse_fpm_binary("no marker").is_none());
    }
}
