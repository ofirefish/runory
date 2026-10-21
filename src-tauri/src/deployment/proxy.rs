use std::time::Duration;

use crate::domain::{AppResult, OperationResult, ReverseProxyConfig, SessionId};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::proxy_render::render_proxy_site;
use super::proxy_validation::{proxy_nginx_conf_name, validate_proxy_config};
use super::service::{ensure_operation, DeploymentService};
use super::validation::validate_app_name;

/// Installs the nginx site read from stdin. Exit 90 = nginx missing. The site is staged in a
/// private temporary file because a proxy has no project directory. A site that fails
/// `nginx -t` is rolled back so an invalid proxy can never take down the other sites.
const INSTALL_SCRIPT: &str = r#"set -e
PATH="$PATH:/usr/sbin:/usr/local/sbin:/sbin"
command -v nginx >/dev/null 2>&1 || exit 90
tmp=$(mktemp)
trap 'rm -f "$tmp" "$tmp.bak" "$tmp.log"' EXIT
cat > "$tmp"
if test "$(id -u)" = 0; then
  SUDO=""
elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  SUDO="sudo -n"
else
  printf 'PRIVILEGE_REQUIRED\n' >&2
  exit 1
fi
site="/etc/nginx/conf.d/$1"
had_site=0
if test -f "$site"; then had_site=1; $SUDO cp "$site" "$tmp.bak"; fi
$SUDO install -m 644 "$tmp" "$site"
if ! $SUDO nginx -t 2>"$tmp.log"; then
  if test "$had_site" = 1; then $SUDO install -m 644 "$tmp.bak" "$site"; else $SUDO rm -f "$site"; fi
  cat "$tmp.log" >&2
  exit 1
fi
if command -v systemctl >/dev/null 2>&1 && test -d /run/systemd/system; then
  if systemctl is-active --quiet nginx; then $SUDO systemctl reload nginx; else $SUDO systemctl start nginx; fi
else
  $SUDO nginx -s reload
fi
printf 'SITE %s\n' "$1"
"#;

impl DeploymentService {
    pub async fn proxy_provision(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: String,
        proxy: ReverseProxyConfig,
    ) -> AppResult<OperationResult> {
        validate_app_name(&name)?;
        validate_proxy_config(&proxy)?;
        let conf_name = proxy_nginx_conf_name(&name)?;
        let site = render_proxy_site(&name, &proxy)?;
        let profile_id = sessions.profile_id(session_id).await?;

        let result = async {
            ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(INSTALL_SCRIPT, vec![conf_name.clone()])
                            .with_stdin(site.into_bytes())?
                            .with_timeout(Duration::from_secs(120)),
                    )
                    .await?,
            )
        }
        .await;

        self.finish(profile_id, "proxy-provision", conf_name, result)
            .await
    }
}
