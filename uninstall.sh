#!/bin/sh

set -eu

INSTALL_DIR="${INSTALL_DIR:-/opt/simadmin}"
SERVICE_NAME="${SERVICE_NAME:-simadmin}"
KEEP_USER_DATA="${KEEP_USER_DATA:-0}"
SYSTEMD_UNIT_DIR="${SIMADMIN_SYSTEMD_UNIT_DIR:-/etc/systemd/system}"
MODEM_RECOVERY_BIN_DIR="${SIMADMIN_MODEM_RECOVERY_BIN_DIR:-/usr/local/bin}"
SIMADMIN_LOCK_DIR="${SIMADMIN_LOCK_DIR:-/run/lock/simadmin-install.lock}"

MODEM_RECOVERY_SERVICE_NAME="${MODEM_RECOVERY_SERVICE_NAME:-simadmin-modem-recovery}"
MODEM_RECOVERY_SCRIPT="${MODEM_RECOVERY_SCRIPT:-${MODEM_RECOVERY_BIN_DIR}/simadmin-modem-recovery.sh}"
NM_CONF="${NM_CONF:-/etc/NetworkManager/conf.d/99-simadmin-unmanaged-modem.conf}"
MM_DEBUG_CONF="${MM_DEBUG_CONF:-${SYSTEMD_UNIT_DIR}/ModemManager.service.d/99-simadmin-debug.conf}"
OTA_STAGING_DIR="${OTA_STAGING_DIR:-/tmp/ota_staging}"
DEVICE_CONFIG_PATH="${DEVICE_CONFIG_PATH:-/data/config.json}"
HUB_AGENT_DB_PATH="${HUB_AGENT_DB_PATH:-/data/hub-agent.db}"

usage() {
  printf '%s\n' \
    'SimAdmin uninstall script' \
    '' \
    'Usage:' \
    '  sh uninstall.sh [options]' \
    '' \
    'Options:' \
    '  --purge                Remove application and user data (default)' \
    '  --keep-user-data       Keep databases, configuration, and backups' \
    '  --install-dir PATH     Installed directory (default: /opt/simadmin)' \
    '  --service-name NAME    Main systemd service name (default: simadmin)' \
    '  -h, --help             Show this help' \
    '' \
    'Environment:' \
    '  INSTALL_DIR=/opt/simadmin' \
    '  SERVICE_NAME=simadmin' \
    '  KEEP_USER_DATA=1       Same as --keep-user-data' \
    '  SIMADMIN_SYSTEMD_UNIT_DIR=/etc/systemd/system' \
    '  SIMADMIN_MODEM_RECOVERY_BIN_DIR=/usr/local/bin' \
    '  SIMADMIN_LOCK_DIR=/run/lock/simadmin-install.lock'
}

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    echo "error: please run as root" >&2
    exit 1
  fi
}

command_exists() {
  command -v "$1" >/dev/null 2>&1
}

normalize_keep_user_data() {
  case "$KEEP_USER_DATA" in
    1|true|TRUE|yes|YES|y|Y) KEEP_USER_DATA=1 ;;
    *) KEEP_USER_DATA=0 ;;
  esac
}

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --purge)
        KEEP_USER_DATA=0
        ;;
      --keep-user-data)
        KEEP_USER_DATA=1
        ;;
      --install-dir)
        option_name="$1"
        shift
        if [ "$#" -eq 0 ]; then
          echo "error: ${option_name} requires a value" >&2
          exit 1
        fi
        INSTALL_DIR="$1"
        ;;
      --install-dir=*)
        INSTALL_DIR="${1#*=}"
        ;;
      --service-name)
        option_name="$1"
        shift
        if [ "$#" -eq 0 ]; then
          echo "error: ${option_name} requires a value" >&2
          exit 1
        fi
        SERVICE_NAME="$1"
        ;;
      --service-name=*)
        SERVICE_NAME="${1#*=}"
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        echo "error: unknown option: $1" >&2
        usage >&2
        exit 1
        ;;
    esac
    shift
  done
}

assert_safe_absolute_dir() {
  config_name="$1"
  config_value="$2"
  case "$config_value" in
    ""|"/"|"/bin"|"/boot"|"/dev"|"/etc"|"/home"|"/opt"|"/proc"|"/root"|"/run"|"/sys"|"/tmp"|"/usr"|"/usr/local"|"/var"|"/data")
      echo "error: unsafe ${config_name}: ${config_value}" >&2
      exit 1
      ;;
    *"/../"*|*"/.."|*"//"*|*[!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_./@+-]*)
      echo "error: invalid ${config_name}: ${config_value}" >&2
      exit 1
      ;;
    /*) ;;
    *)
      echo "error: ${config_name} must be an absolute path: ${config_value}" >&2
      exit 1
      ;;
  esac
}

assert_safe_service_name() {
  config_name="$1"
  config_value="$2"
  case "$config_value" in
    ""|*/*|*..*|*[!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.@-]*)
      echo "error: unsafe ${config_name}: ${config_value}" >&2
      exit 1
      ;;
  esac
}

assert_managed_file() {
  config_name="$1"
  config_value="$2"
  expected_name="$3"
  case "$config_value" in
    /*) ;;
    *)
      echo "error: ${config_name} must be an absolute path: ${config_value}" >&2
      exit 1
      ;;
  esac
  case "$config_value" in
    *"/../"*|*"/.."|*"//"*|*[!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_./@+-]*)
      echo "error: invalid ${config_name}: ${config_value}" >&2
      exit 1
      ;;
  esac
  if [ "${config_value##*/}" != "$expected_name" ]; then
    echo "error: unsafe ${config_name}: expected basename ${expected_name}" >&2
    exit 1
  fi
  managed_parent="${config_value%/*}"
  case "$managed_parent" in
    ""|"/")
      echo "error: unsafe ${config_name} parent: ${managed_parent}" >&2
      exit 1
      ;;
  esac
}

validate_configuration() {
  assert_safe_absolute_dir INSTALL_DIR "$INSTALL_DIR"
  assert_safe_absolute_dir SYSTEMD_UNIT_DIR "$SYSTEMD_UNIT_DIR"
  assert_safe_absolute_dir MODEM_RECOVERY_BIN_DIR "$MODEM_RECOVERY_BIN_DIR"
  assert_safe_absolute_dir SIMADMIN_LOCK_DIR "$SIMADMIN_LOCK_DIR"
  assert_safe_absolute_dir OTA_STAGING_DIR "$OTA_STAGING_DIR"
  assert_safe_service_name SERVICE_NAME "$SERVICE_NAME"
  assert_safe_service_name MODEM_RECOVERY_SERVICE_NAME "$MODEM_RECOVERY_SERVICE_NAME"
  assert_managed_file MODEM_RECOVERY_SCRIPT "$MODEM_RECOVERY_SCRIPT" simadmin-modem-recovery.sh
  assert_managed_file NM_CONF "$NM_CONF" 99-simadmin-unmanaged-modem.conf
  assert_managed_file MM_DEBUG_CONF "$MM_DEBUG_CONF" 99-simadmin-debug.conf
  assert_managed_file DEVICE_CONFIG_PATH "$DEVICE_CONFIG_PATH" config.json
  assert_managed_file HUB_AGENT_DB_PATH "$HUB_AGENT_DB_PATH" hub-agent.db
  for managed_dir in "$INSTALL_DIR" "$SYSTEMD_UNIT_DIR" "$MODEM_RECOVERY_BIN_DIR"; do
    if [ -L "$managed_dir" ]; then
      echo "error: refusing to uninstall through symbolic-link directory: ${managed_dir}" >&2
      exit 1
    fi
  done
}

cleanup_uninstaller() {
  owned_lock_pid="$(cat "${SIMADMIN_LOCK_DIR}/pid" 2>/dev/null || true)"
  if [ "${uninstall_lock_owned:-0}" -eq 1 ] && [ "$owned_lock_pid" = "$$" ] \
    && [ -d "$SIMADMIN_LOCK_DIR" ]; then
    rm -rf -- "$SIMADMIN_LOCK_DIR"
  fi
}

uninstaller_exit() {
  exit_status="$1"
  trap - EXIT INT TERM
  cleanup_uninstaller
  exit "$exit_status"
}

acquire_install_lock() {
  lock_parent="${SIMADMIN_LOCK_DIR%/*}"
  [ -n "$lock_parent" ] || lock_parent="/"
  mkdir -p "$lock_parent"
  if mkdir "$SIMADMIN_LOCK_DIR" 2>/dev/null; then
    uninstall_lock_owned=1
    printf '%s\n' "$$" > "${SIMADMIN_LOCK_DIR}/pid"
    return 0
  fi

  lock_pid="$(cat "${SIMADMIN_LOCK_DIR}/pid" 2>/dev/null || true)"
  case "$lock_pid" in
    ""|*[!0-9]*) lock_pid="" ;;
  esac
  if [ -n "$lock_pid" ] && kill -0 "$lock_pid" 2>/dev/null; then
    echo "error: another SimAdmin install or uninstall is running (pid ${lock_pid})" >&2
    exit 1
  fi

  stale_lock="${SIMADMIN_LOCK_DIR}.stale.$$"
  rm -rf -- "$stale_lock"
  if ! mv "$SIMADMIN_LOCK_DIR" "$stale_lock" 2>/dev/null; then
    echo "error: failed to take over stale install lock; retry the operation" >&2
    exit 1
  fi
  moved_lock_pid="$(cat "${stale_lock}/pid" 2>/dev/null || true)"
  case "$moved_lock_pid" in
    ""|*[!0-9]*) moved_lock_pid="" ;;
  esac
  if [ "$moved_lock_pid" != "$lock_pid" ]; then
    if [ ! -e "$SIMADMIN_LOCK_DIR" ]; then
      mv "$stale_lock" "$SIMADMIN_LOCK_DIR" 2>/dev/null || true
    fi
    echo "error: install lock changed while it was being checked; retry the operation" >&2
    exit 1
  fi
  echo "==> removing stale install lock"
  rm -rf -- "$stale_lock"
  if ! mkdir "$SIMADMIN_LOCK_DIR" 2>/dev/null; then
    echo "error: another SimAdmin install or uninstall acquired the lock" >&2
    exit 1
  fi
  uninstall_lock_owned=1
  printf '%s\n' "$$" > "${SIMADMIN_LOCK_DIR}/pid"
}

remove_path() {
  path="$1"
  if [ -e "$path" ] || [ -L "$path" ]; then
    echo "==> removing ${path}"
    rm -rf -- "$path"
    return 0
  fi
  return 1
}

stop_disable_service() {
  unit="$1"
  command_exists systemctl || return 0

  if systemctl is-active --quiet "$unit" 2>/dev/null; then
    echo "==> stopping ${unit}"
    systemctl stop "$unit" >/dev/null 2>&1 || true
  fi
  if systemctl is-enabled --quiet "$unit" 2>/dev/null; then
    echo "==> disabling ${unit}"
    systemctl disable "$unit" >/dev/null 2>&1 || true
  fi
}

remove_systemd_unit() {
  unit="$1"
  unit_changed=0
  if remove_path "${SYSTEMD_UNIT_DIR}/multi-user.target.wants/${unit}"; then
    unit_changed=1
  fi
  if remove_path "${SYSTEMD_UNIT_DIR}/${unit}"; then
    unit_changed=1
  fi
  [ "$unit_changed" -eq 1 ]
}

cleanup_transaction_residue() {
  for transaction_path in \
    "${INSTALL_DIR}"/.simadmin.new.* \
    "${INSTALL_DIR}"/.simadmin.previous.* \
    "${INSTALL_DIR}"/.www.new.* \
    "${INSTALL_DIR}"/.www.previous.* \
    "${INSTALL_DIR}"/.meta.json.new.* \
    "${INSTALL_DIR}"/.meta.json.previous.* \
    "${SYSTEMD_UNIT_DIR}"/."${SERVICE_NAME}".service.new.* \
    "${SYSTEMD_UNIT_DIR}"/."${SERVICE_NAME}".service.previous.* \
    "${SYSTEMD_UNIT_DIR}"/.simadmin-modem-recovery.service.new.* \
    "${SYSTEMD_UNIT_DIR}"/.simadmin-modem-recovery.service.previous.* \
    "${MODEM_RECOVERY_BIN_DIR}"/.simadmin-modem-recovery.sh.new.* \
    "${MODEM_RECOVERY_BIN_DIR}"/.simadmin-modem-recovery.sh.previous.*; do
    if [ -e "$transaction_path" ] || [ -L "$transaction_path" ]; then
      remove_path "$transaction_path" || true
    fi
  done
}

remove_install_files_keep_data() {
  remove_path "${INSTALL_DIR}/simadmin" || true
  remove_path "${INSTALL_DIR}/www" || true
  remove_path "${INSTALL_DIR}/lpac" || true
  remove_path "${INSTALL_DIR}/meta.json" || true

  if [ -d "$INSTALL_DIR" ]; then
    if rmdir "$INSTALL_DIR" >/dev/null 2>&1; then
      echo "==> removed empty install dir ${INSTALL_DIR}"
    else
      echo "==> kept user data under ${INSTALL_DIR}"
    fi
  fi
  if [ -f "$DEVICE_CONFIG_PATH" ]; then
    echo "==> kept user config ${DEVICE_CONFIG_PATH}"
  fi
}

remove_install_files_purge() {
  remove_path "$INSTALL_DIR" || true
  remove_path "$DEVICE_CONFIG_PATH" || true
  remove_path "$HUB_AGENT_DB_PATH" || true
  remove_path "${HUB_AGENT_DB_PATH}-wal" || true
  remove_path "${HUB_AGENT_DB_PATH}-shm" || true
  remove_path "${HUB_AGENT_DB_PATH}-journal" || true
}

main() {
  parse_args "$@"
  normalize_keep_user_data
  require_root
  validate_configuration
  acquire_install_lock
  trap 'uninstaller_exit $?' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  echo "==> uninstalling SimAdmin"
  if [ "$KEEP_USER_DATA" -eq 1 ]; then
    echo "==> mode: keep user data"
  else
    echo "==> mode: purge all data"
  fi

  stop_disable_service "${SERVICE_NAME}.service"
  stop_disable_service "${MODEM_RECOVERY_SERVICE_NAME}.service"
  stop_disable_service "simadmin-secondary-qmi.service"

  systemd_changed=0
  if remove_systemd_unit "${SERVICE_NAME}.service"; then
    systemd_changed=1
  fi
  if remove_systemd_unit "${MODEM_RECOVERY_SERVICE_NAME}.service"; then
    systemd_changed=1
  fi
  if remove_systemd_unit "simadmin-secondary-qmi.service"; then
    systemd_changed=1
  fi

  remove_path "$MODEM_RECOVERY_SCRIPT" || true
  remove_path "/etc/udev/rules.d/99-simadmin-secondary-qmi.rules" || true
  remove_path "/etc/udev/rules.d/simadmin-secondary-qmi.rules" || true
  remove_path "/run/udev/rules.d/99-simadmin-secondary-qmi.rules" || true
  remove_path "/run/udev/rules.d/simadmin-secondary-qmi.rules" || true

  nm_changed=0
  if remove_path "$NM_CONF"; then
    nm_changed=1
  fi

  mm_changed=0
  if remove_path "$MM_DEBUG_CONF"; then
    mm_changed=1
    mm_override_dir="${MM_DEBUG_CONF%/*}"
    rmdir "$mm_override_dir" >/dev/null 2>&1 || true
  fi

  remove_path "$OTA_STAGING_DIR" || true
  cleanup_transaction_residue

  if [ "$KEEP_USER_DATA" -eq 1 ]; then
    remove_install_files_keep_data
  else
    remove_install_files_purge
  fi

  if command_exists systemctl; then
    if [ "$systemd_changed" -eq 1 ] || [ "$mm_changed" -eq 1 ]; then
      echo "==> reloading systemd"
      systemctl daemon-reload >/dev/null 2>&1 || true
    fi
    systemctl reset-failed "${SERVICE_NAME}.service" >/dev/null 2>&1 || true
    systemctl reset-failed "${MODEM_RECOVERY_SERVICE_NAME}.service" >/dev/null 2>&1 || true

    if [ "$nm_changed" -eq 1 ] && systemctl is-active --quiet NetworkManager.service; then
      echo "==> restarting NetworkManager"
      systemctl restart NetworkManager.service || true
    fi
    if [ "$mm_changed" -eq 1 ] && systemctl is-active --quiet ModemManager.service; then
      echo "==> restarting ModemManager"
      systemctl restart ModemManager.service || true
    fi
  fi

  echo "==> system dependencies were left installed"
  echo "==> done"
}

if [ "${SIMADMIN_UNINSTALL_LIBRARY_ONLY:-0}" != "1" ]; then
  main "$@"
fi
