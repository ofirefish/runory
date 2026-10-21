use std::time::Duration;

use crate::domain::{AppResult, HtmlProjectConfig, OperationResult, SessionId};
use crate::ssh::{RemoteCommand, ServerSessionManager};

use super::html_render::render_html_site;
use super::html_validation::{html_nginx_conf_name, validate_html_config, validate_html_path};
use super::service::{ensure_operation, DeploymentService};
use super::validation::validate_app_name;

/// Installs the nginx site read from stdin. Exit 90 = nginx missing, 91 = project or web root
/// missing. A site that fails `nginx -t` is rolled back so an invalid project can never take
/// down the other sites served by the same nginx.
const INSTALL_SCRIPT: &str = r#"set -e
PATH="$PATH:/usr/sbin:/usr/local/sbin:/sbin"
command -v nginx >/dev/null 2>&1 || exit 90
test -d "$1" || exit 91
if test -n "$2"; then test -d "$1/$2" || exit 91; fi
dir="$1/.runory"
mkdir -p "$dir"
cat > "$dir/$3"
if test "$(id -u)" = 0; then
  SUDO=""
elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  SUDO="sudo -n"
else
  printf 'SITE_PENDING %s\n' "$dir/$3"
  exit 1
fi
site="/etc/nginx/conf.d/$3"
had_site=0
if test -f "$site"; then had_site=1; $SUDO cp "$site" "$dir/$3.bak"; fi
$SUDO cp "$dir/$3" "$site"
if ! $SUDO nginx -t 2>"$dir/nginx-test.log"; then
  if test "$had_site" = 1; then $SUDO cp "$dir/$3.bak" "$site"; else $SUDO rm -f "$site"; fi
  cat "$dir/nginx-test.log" >&2
  exit 1
fi
if command -v systemctl >/dev/null 2>&1 && test -d /run/systemd/system; then
  if systemctl is-active --quiet nginx; then $SUDO systemctl reload nginx; else $SUDO systemctl start nginx; fi
else
  $SUDO nginx -s reload
fi
printf 'SITE %s\n' "$3"
"#;

impl DeploymentService {
    pub async fn html_provision(
        &self,
        sessions: &ServerSessionManager,
        session_id: SessionId,
        name: String,
        project_path: String,
        html: HtmlProjectConfig,
    ) -> AppResult<OperationResult> {
        validate_app_name(&name)?;
        validate_html_path(&project_path)?;
        validate_html_config(&html)?;
        let conf_name = html_nginx_conf_name(&name)?;
        let site = render_html_site(&name, &project_path, &html)?;
        let profile_id = sessions.profile_id(session_id).await?;

        let result = async {
            ensure_operation(
                sessions
                    .exec(
                        session_id,
                        RemoteCommand::script(
                            INSTALL_SCRIPT,
                            vec![project_path.clone(), html.root_dir.clone(), conf_name],
                        )
                        .with_stdin(site.into_bytes())?
                        .with_timeout(Duration::from_secs(120)),
                    )
                    .await?,
            )
        }
        .await;

        self.finish(profile_id, "html-provision", project_path, result)
            .await
    }
}
