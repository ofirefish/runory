/// Installs and starts a Runory-generated systemd unit read from stdin (args: project dir, unit name, enable flag).
/// Without root or passwordless sudo the unit is left in
/// `<project>/.runory` and reported as pending instead of failing silently.
pub(super) const INSTALL_UNIT_SCRIPT: &str = r#"set -e
dir="$1/.runory"
mkdir -p "$dir"
cat > "$dir/$2"
if test "$(id -u)" = 0; then
  SUDO=""
elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  SUDO="sudo -n"
else
  printf 'UNIT_PENDING %s\n' "$dir/$2"
  exit 1
fi
$SUDO cp "$dir/$2" "/etc/systemd/system/$2"
$SUDO systemctl daemon-reload
if test "$3" = 1; then $SUDO systemctl enable "$2"; else $SUDO systemctl disable "$2"; fi
$SUDO systemctl restart "$2"
sleep 2
systemctl is-active --quiet "$2"
printf 'SERVICE %s\n' "$2"
"#;
