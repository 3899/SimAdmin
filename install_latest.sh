#!/bin/sh

set -eu

REPO="${REPO:-3899/SimAdmin}"
INSTALL_DIR="${INSTALL_DIR:-/opt/simadmin}"
SERVICE_NAME="${SERVICE_NAME:-simadmin}"
SYSTEMD_UNIT_DIR="${SIMADMIN_SYSTEMD_UNIT_DIR:-/etc/systemd/system}"
MODEM_RECOVERY_BIN_DIR="${SIMADMIN_MODEM_RECOVERY_BIN_DIR:-/usr/local/bin}"
VERSION="${VERSION:-latest}"
GH_PROXY="${GH_PROXY:-https://gh-proxy.com/}"
GH_PROXY_FALLBACKS="${GH_PROXY_FALLBACKS:-https://ghproxy.net/ https://githubproxy.cc/}"
RAW_BASE="${RAW_BASE:-https://raw.githubusercontent.com/${REPO}}"
SERVICE_URL="${SERVICE_URL:-}"
MODEM_RECOVERY_SCRIPT_URL="${MODEM_RECOVERY_SCRIPT_URL:-}"
MODEM_RECOVERY_SERVICE_URL="${MODEM_RECOVERY_SERVICE_URL:-}"
ASSET_URL="${ASSET_URL:-}"
WFC="${WFC:-0}"
VARIANT="${VARIANT:-}"
ASSET_NAME="${ASSET_NAME:-}"
SIMADMIN_TARGET_ARCH="${SIMADMIN_TARGET_ARCH:-}"
SIMADMIN_INSTALL_SYSTEM_DEPS="${SIMADMIN_INSTALL_SYSTEM_DEPS:-1}"
SIMADMIN_DEPS_MODE="${SIMADMIN_DEPS_MODE:-auto}"
SIMADMIN_APT_UPDATE="${SIMADMIN_APT_UPDATE:-auto}"
SIMADMIN_MODEM_PROTOCOL="${SIMADMIN_MODEM_PROTOCOL:-auto}"
SIMADMIN_ENABLE_NETWORKMANAGER="${SIMADMIN_ENABLE_NETWORKMANAGER:-1}"
SIMADMIN_REFRESH_MODEM_DEVICES="${SIMADMIN_REFRESH_MODEM_DEVICES:-auto}"
SIMADMIN_INSTALL_LPAC="${SIMADMIN_INSTALL_LPAC:-1}"
SIMADMIN_LPAC_ONLY="${SIMADMIN_LPAC_ONLY:-0}"
SIMADMIN_VERIFY_ASSET="${SIMADMIN_VERIFY_ASSET:-auto}"
SIMADMIN_ASSET_SHA256="${SIMADMIN_ASSET_SHA256:-}"
SIMADMIN_TMPDIR="${SIMADMIN_TMPDIR:-}"
SIMADMIN_LOCK_DIR="${SIMADMIN_LOCK_DIR:-/run/lock/simadmin-install.lock}"
SIMADMIN_HEALTH_URL="${SIMADMIN_HEALTH_URL:-http://127.0.0.1:3000/api/health}"
SIMADMIN_HEALTH_RETRIES="${SIMADMIN_HEALTH_RETRIES:-15}"
SIMADMIN_SKIP_HEALTHCHECK="${SIMADMIN_SKIP_HEALTHCHECK:-0}"
SIMADMIN_SKIP_ELF_CHECK="${SIMADMIN_SKIP_ELF_CHECK:-0}"
SIMADMIN_MIN_MODEMMANAGER_VERSION="${SIMADMIN_MIN_MODEMMANAGER_VERSION:-}"
SIMADMIN_MIN_NETWORKMANAGER_VERSION="${SIMADMIN_MIN_NETWORKMANAGER_VERSION:-}"
LPAC_REPO="${LPAC_REPO:-estkme-group/lpac}"
LPAC_RELEASE_BASE_URL="${LPAC_RELEASE_BASE_URL:-https://github.com/${LPAC_REPO}/releases/latest/download}"
LPAC_LATEST_RELEASE_URL="${LPAC_LATEST_RELEASE_URL:-https://github.com/${LPAC_REPO}/releases/latest}"
LPAC_COMPAT_RELEASE_BASE_URL="${LPAC_COMPAT_RELEASE_BASE_URL:-https://github.com/${REPO}/releases/download/lpac}"
LPAC_COMPAT_MANIFEST_NAME="${LPAC_COMPAT_MANIFEST_NAME:-lpac.json}"
LPAC_TARGET_ARCH="${LPAC_TARGET_ARCH:-}"
LPAC_TARGET_VERSION="${LPAC_TARGET_VERSION:-}"
LPAC_LATEST_RELEASE_API_URL="${LPAC_LATEST_RELEASE_API_URL:-https://api.github.com/repos/${LPAC_REPO}/releases/latest}"
LPAC_ASSET_FLAVOR="${LPAC_ASSET_FLAVOR:-compat}"
LPAC_ASSET_NAME="${LPAC_ASSET_NAME:-}"
LPAC_ASSET_URL="${LPAC_ASSET_URL:-}"

truthy() {
  case "$1" in
    1|true|TRUE|yes|YES|y|Y|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

normalize_asset_name() {
  case "$1" in
    volte|simadmin-volte|simadmin-volte.tar.gz)
      printf '%s\n' "volte"
      ;;
    vowifi|simadmin-vowifi|simadmin-vowifi.tar.gz)
      printf '%s\n' "vowifi"
      ;;
    full|all|simadmin-full|simadmin-full.tar.gz)
      printf '%s\n' "full"
      ;;
    wfc|simadmin-wfc|simadmin-wfc.tar.gz)
      printf '%s\n' "vowifi"
      ;;
    ""|default|standard|simadmin|simadmin.tar.gz)
      printf '%s\n' ""
      ;;
    *.tar.gz)
      printf '%s\n' "$1"
      ;;
    *)
      printf '%s.tar.gz\n' "$1"
      ;;
  esac
}

select_asset_name() {
  selected_asset="$(normalize_asset_name "$1")"
  case "$selected_asset" in
    volte|vowifi|full)
      VARIANT="$selected_asset"
      ASSET_NAME=""
      ;;
    wfc)
      WFC=1
      VARIANT="vowifi"
      ASSET_NAME=""
      ;;
    "")
      VARIANT=""
      ASSET_NAME=""
      ;;
    *)
      ASSET_NAME="$selected_asset"
      ;;
  esac
}

if truthy "$WFC" || [ "$VARIANT" = "wfc" ]; then
  WFC=1
  VARIANT="vowifi"
fi
if [ -n "${ASSET_NAME:-}" ]; then
  select_asset_name "$ASSET_NAME"
fi

usage() {
  printf '%s\n' \
    'SimAdmin install / upgrade script' \
    '' \
    'Usage:' \
    '  sh install_latest.sh [options] [version]' \
    '' \
    'Examples:' \
    '  sh install_latest.sh                        # Install latest standard release' \
    '  sh install_latest.sh --volte                # Install latest VoLTE release' \
    '  sh install_latest.sh --vowifi               # Install latest VoWiFi release' \
    '  sh install_latest.sh --full                 # Install latest Full release' \
    '  sh install_latest.sh --wfc                  # Install latest Wi-Fi Calling release' \
    '  sh install_latest.sh -v1.2.0 --volte        # Install v1.2.0 VoLTE release' \
    '  curl -fsSL .../install_latest.sh | WFC=1 sh # Install latest WFC release via env' \
    '' \
    'Options:' \
    '  -v, --version VERSION  Target version to install (default: latest)' \
    '  --volte                Install VoLTE release asset' \
    '  --vowifi               Install VoWiFi release asset' \
    '  --full                 Install Full release asset' \
    '  --wfc                  Install Wi-Fi Calling release asset (alias for VoWiFi)' \
    '  -a, --asset NAME       Specify release asset (e.g. volte, vowifi, full, wfc)' \
    '  --install-dir PATH     Installation directory (default: /opt/simadmin)' \
    '  --service-name NAME    Main systemd service name (default: simadmin)' \
    '  --deps-mode MODE       Dependency mode: auto, minimal, full, skip' \
    '  --modem-protocol MODE  Modem protocol: auto, qmi, mbim, at, all' \
    '  --refresh-modem        Force udev refresh and ModemManager restart' \
    '  --no-refresh-modem     Do not refresh modem devices' \
    '  --no-lpac              Skip lpac installation' \
    '  --lpac-only            Install or update only the shared lpac runtime' \
    '  -h, --help             Show this help' \
    '' \
    'Environment Variables:' \
    '  VERSION=latest         Specify version' \
    '  VARIANT=volte|vowifi|full|wfc  Specify release variant' \
    '  WFC=1                  Install Wi-Fi Calling / VoWiFi release' \
    '  ASSET_NAME=...         Specify release asset filename' \
    '  INSTALL_DIR=/opt/simadmin' \
    '  SERVICE_NAME=simadmin' \
    '  SIMADMIN_DEPS_MODE=auto (auto, minimal, full, skip)' \
    '  SIMADMIN_APT_UPDATE=auto (auto, always, never)' \
    '  SIMADMIN_MODEM_PROTOCOL=auto (auto, qmi, mbim, at, all)' \
    '  SIMADMIN_INSTALL_LPAC=1 (set to 0 to skip lpac)' \
    '  GH_PROXY=https://gh-proxy.com/'
}

parse_args() {
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -v|--version)
        shift
        if [ "$#" -eq 0 ]; then
          echo "error: --version requires a value" >&2
          exit 1
        fi
        VERSION="$1"
        ;;
      -v=*|--version=*)
        VERSION="${1#*=}"
        ;;
      -v*)
        VERSION="${1#-v}"
        ;;
      --volte)
        VARIANT="volte"
        ;;
      --vowifi)
        VARIANT="vowifi"
        ;;
      --full|--all)
        VARIANT="full"
        ;;
      --wfc)
        WFC=1
        VARIANT="wfc"
        ;;
      -a|--asset|--variant)
        option_name="$1"
        shift
        if [ "$#" -eq 0 ]; then
          echo "error: ${option_name} requires a value" >&2
          exit 1
        fi
        select_asset_name "$1"
        ;;
      -a=*|--asset=*|--variant=*)
        select_asset_name "${1#*=}"
        ;;
      -a*)
        select_asset_name "${1#-a}"
        ;;
      --install-dir)
        shift
        if [ "$#" -eq 0 ]; then
          echo "error: --install-dir requires a value" >&2
          exit 1
        fi
        INSTALL_DIR="$1"
        ;;
      --install-dir=*)
        INSTALL_DIR="${1#*=}"
        ;;
      --service-name)
        shift
        if [ "$#" -eq 0 ]; then
          echo "error: --service-name requires a value" >&2
          exit 1
        fi
        SERVICE_NAME="$1"
        ;;
      --service-name=*)
        SERVICE_NAME="${1#*=}"
        ;;
      --deps-mode)
        shift
        if [ "$#" -eq 0 ]; then
          echo "error: --deps-mode requires a value" >&2
          exit 1
        fi
        SIMADMIN_DEPS_MODE="$1"
        ;;
      --deps-mode=*)
        SIMADMIN_DEPS_MODE="${1#*=}"
        ;;
      --modem-protocol)
        shift
        if [ "$#" -eq 0 ]; then
          echo "error: --modem-protocol requires a value" >&2
          exit 1
        fi
        SIMADMIN_MODEM_PROTOCOL="$1"
        ;;
      --modem-protocol=*)
        SIMADMIN_MODEM_PROTOCOL="${1#*=}"
        ;;
      --refresh-modem)
        SIMADMIN_REFRESH_MODEM_DEVICES=1
        ;;
      --no-refresh-modem)
        SIMADMIN_REFRESH_MODEM_DEVICES=0
        ;;
      --no-lpac|--skip-lpac)
        SIMADMIN_INSTALL_LPAC=0
        ;;
      --lpac-only)
        SIMADMIN_LPAC_ONLY=1
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      -*)
        echo "error: unknown option: $1" >&2
        usage >&2
        exit 1
        ;;
      *)
        VERSION="$1"
        ;;
    esac
    shift
  done
}

require_root() {
  if [ "$(id -u)" -ne 0 ]; then
    echo "error: please run as root" >&2
    exit 1
  fi
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: missing required command: $1" >&2
    exit 1
  fi
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
  case "$SERVICE_NAME" in
    ""|*/*|*..*|*[!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.@-]*)
      echo "error: unsafe SERVICE_NAME: ${SERVICE_NAME}" >&2
      exit 1
      ;;
  esac
}

validate_configuration() {
  assert_safe_absolute_dir INSTALL_DIR "$INSTALL_DIR"
  assert_safe_absolute_dir SYSTEMD_UNIT_DIR "$SYSTEMD_UNIT_DIR"
  assert_safe_absolute_dir MODEM_RECOVERY_BIN_DIR "$MODEM_RECOVERY_BIN_DIR"
  assert_safe_absolute_dir SIMADMIN_LOCK_DIR "$SIMADMIN_LOCK_DIR"
  assert_safe_service_name
  for managed_dir in "$INSTALL_DIR" "$SYSTEMD_UNIT_DIR" "$MODEM_RECOVERY_BIN_DIR"; do
    if [ -L "$managed_dir" ]; then
      echo "error: refusing to install through symbolic-link directory: ${managed_dir}" >&2
      exit 1
    fi
  done

  case "$SIMADMIN_DEPS_MODE" in auto|minimal|full|skip) ;; *)
    echo "error: invalid SIMADMIN_DEPS_MODE: ${SIMADMIN_DEPS_MODE}" >&2
    exit 1
  esac
  case "$SIMADMIN_APT_UPDATE" in auto|always|never) ;; *)
    echo "error: invalid SIMADMIN_APT_UPDATE: ${SIMADMIN_APT_UPDATE}" >&2
    exit 1
  esac
  case "$SIMADMIN_MODEM_PROTOCOL" in auto|qmi|mbim|at|all) ;; *)
    echo "error: invalid SIMADMIN_MODEM_PROTOCOL: ${SIMADMIN_MODEM_PROTOCOL}" >&2
    exit 1
  esac
  case "$SIMADMIN_REFRESH_MODEM_DEVICES" in auto|0|1|false|FALSE|no|NO|off|OFF|true|TRUE|yes|YES|on|ON) ;; *)
    echo "error: invalid SIMADMIN_REFRESH_MODEM_DEVICES: ${SIMADMIN_REFRESH_MODEM_DEVICES}" >&2
    exit 1
  esac
  case "$SIMADMIN_HEALTH_RETRIES" in ""|*[!0-9]*)
    echo "error: SIMADMIN_HEALTH_RETRIES must be a non-negative integer" >&2
    exit 1
  esac
}

validate_running_architecture() {
  machine_arch="$(normalize_simadmin_arch "$(uname -m)" || true)"
  if [ -z "$machine_arch" ]; then
    echo "error: unsupported machine architecture: $(uname -m)" >&2
    exit 1
  fi
  if [ -n "$SIMADMIN_TARGET_ARCH" ]; then
    requested_arch="$(normalize_simadmin_arch "$SIMADMIN_TARGET_ARCH" || true)"
    if [ -z "$requested_arch" ] || [ "$requested_arch" != "$machine_arch" ]; then
      echo "error: SIMADMIN_TARGET_ARCH=${SIMADMIN_TARGET_ARCH} does not match this machine ($(uname -m))" >&2
      exit 1
    fi
  fi
  RUNTIME_ARCH="$machine_arch"
}

cleanup_installer() {
  if [ "${TRANSACTION_ACTIVE:-0}" -eq 1 ]; then
    rollback_install || true
  fi
  cleanup_staged_transaction || true
  if [ -n "${tmp_dir:-}" ] && [ -d "$tmp_dir" ]; then
    rm -rf -- "$tmp_dir"
  fi
  owned_lock_pid="$(cat "${SIMADMIN_LOCK_DIR}/pid" 2>/dev/null || true)"
  if [ "${install_lock_owned:-0}" -eq 1 ] && [ "$owned_lock_pid" = "$$" ] \
    && [ -d "$SIMADMIN_LOCK_DIR" ]; then
    rm -rf -- "$SIMADMIN_LOCK_DIR"
  fi
}

installer_exit() {
  exit_status="$1"
  trap - EXIT INT TERM
  cleanup_installer
  exit "$exit_status"
}

acquire_install_lock() {
  lock_parent="${SIMADMIN_LOCK_DIR%/*}"
  [ -n "$lock_parent" ] || lock_parent="/"
  mkdir -p "$lock_parent"
  if mkdir "$SIMADMIN_LOCK_DIR" 2>/dev/null; then
    install_lock_owned=1
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
  install_lock_owned=1
  printf '%s\n' "$$" > "${SIMADMIN_LOCK_DIR}/pid"
}

create_temp_dir() {
  for temp_base in "${SIMADMIN_TMPDIR:-}" "${TMPDIR:-}" /tmp /var/tmp; do
    [ -n "$temp_base" ] || continue
    case "$temp_base" in /*) ;; *) continue ;; esac
    if [ ! -d "$temp_base" ] && [ "$temp_base" = "/tmp" ]; then
      mkdir -p /tmp
      chmod 1777 /tmp
    fi
    [ -d "$temp_base" ] && [ -w "$temp_base" ] || continue
    if created_temp="$(TMPDIR="$temp_base" mktemp -d "${temp_base%/}/simadmin.XXXXXXXXXX" 2>/dev/null)"; then
      printf '%s\n' "$created_temp"
      return 0
    fi
  done
  echo "error: no writable temporary directory is available" >&2
  return 1
}

minimum_package_version() {
  case "$1" in
    modemmanager) printf '%s\n' "$SIMADMIN_MIN_MODEMMANAGER_VERSION" ;;
    network-manager) printf '%s\n' "$SIMADMIN_MIN_NETWORKMANAGER_VERSION" ;;
    *) printf '%s\n' "" ;;
  esac
}

installed_package_version() {
  package_name="$1"
  command -v dpkg-query >/dev/null 2>&1 || return 1
  package_status="$(dpkg-query -W -f='${Status}\n' "$package_name" 2>/dev/null || true)"
  [ "$package_status" = "install ok installed" ] || return 1
  dpkg-query -W -f='${Version}\n' "$package_name" 2>/dev/null
}

package_requirement_satisfied() {
  package_name="$1"
  installed_version="$(installed_package_version "$package_name" || true)"
  [ -n "$installed_version" ] || return 1
  minimum_version="$(minimum_package_version "$package_name")"
  if [ -n "$minimum_version" ]; then
    command -v dpkg >/dev/null 2>&1 || return 1
    dpkg --compare-versions "$installed_version" ge "$minimum_version"
  fi
}

missing_package_list() {
  requested_packages="$1"
  missing_result=""
  for package_name in $requested_packages; do
    if package_requirement_satisfied "$package_name"; then
      continue
    fi
    missing_result="${missing_result}${missing_result:+ }${package_name}"
  done
  printf '%s\n' "$missing_result"
}

essential_package_list() {
  packages="ca-certificates curl dbus modemmanager tar udev"
  if truthy "$SIMADMIN_ENABLE_NETWORKMANAGER"; then
    packages="$packages network-manager"
  fi
  printf '%s\n' "$packages"
}

ensure_package_list() {
  dependency_label="$1"
  requested_packages="$2"
  allow_optional_failure="${3:-0}"
  [ -n "$requested_packages" ] || return 0
  missing_packages="$(missing_package_list "$requested_packages")"

  if [ -z "$missing_packages" ]; then
    echo "==> ${dependency_label} already satisfy requirements"
    if [ "$SIMADMIN_APT_UPDATE" = "always" ] && [ "${APT_METADATA_UPDATED:-0}" -ne 1 ]; then
      command -v apt-get >/dev/null 2>&1 || {
        echo "error: apt-get is required by SIMADMIN_APT_UPDATE=always" >&2
        exit 1
      }
      echo "==> updating apt package metadata"
      apt-get update
      APT_METADATA_UPDATED=1
    fi
    return 0
  fi

  command -v apt-get >/dev/null 2>&1 || {
    if [ "$allow_optional_failure" -eq 1 ]; then
      essential_remaining="$(missing_package_list "$(essential_package_list)")"
      if [ -z "$essential_remaining" ]; then
        echo "warning: apt-get is unavailable; continuing without optional packages: ${missing_packages}" >&2
        return 0
      fi
    fi
    echo "error: apt-get is unavailable; missing packages: ${missing_packages}" >&2
    exit 1
  }
  if [ "$SIMADMIN_APT_UPDATE" != "never" ] && [ "${APT_METADATA_UPDATED:-0}" -ne 1 ]; then
    echo "==> updating apt package metadata"
    apt-get update || true
    APT_METADATA_UPDATED=1
  fi

  echo "==> installing missing ${dependency_label}: ${missing_packages}"
  # Word splitting is intentional: Debian package names cannot contain spaces.
  if ! DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends $missing_packages; then
    if [ "$allow_optional_failure" -eq 1 ]; then
      echo "warning: apt-get encountered errors while installing: ${missing_packages}" >&2
    fi
  fi
  SYSTEM_DEPS_CHANGED=1

  remaining_packages="$(missing_package_list "$requested_packages")"
  if [ -n "$remaining_packages" ]; then
    if [ "$allow_optional_failure" -eq 1 ]; then
      essential_remaining="$(missing_package_list "$(essential_package_list)")"
      if [ -z "$essential_remaining" ]; then
        echo "warning: optional packages are missing but core requirements are satisfied: ${remaining_packages}" >&2
        return 0
      fi
      echo "error: essential dependencies are still missing: ${essential_remaining}" >&2
      exit 1
    fi
    echo "error: dependencies are still missing or below their required versions: ${remaining_packages}" >&2
    exit 1
  fi
}

detect_modem_protocols() {
  case "$SIMADMIN_MODEM_PROTOCOL" in
    qmi|mbim|at) printf '%s\n' "$SIMADMIN_MODEM_PROTOCOL"; return 0 ;;
    all) printf '%s\n' "qmi mbim"; return 0 ;;
  esac

  detected_protocols=""
  for protocol_node in /dev/wwan*qmi* /dev/*qmi*; do
    if [ -e "$protocol_node" ]; then
      detected_protocols="qmi"
      break
    fi
  done
  for protocol_node in /dev/wwan*mbim* /dev/*mbim*; do
    if [ -e "$protocol_node" ]; then
      detected_protocols="${detected_protocols}${detected_protocols:+ }mbim"
      break
    fi
  done

  if command -v mmcli >/dev/null 2>&1; then
    modem_description="$(mmcli -m any 2>/dev/null || true)"
    case "$modem_description" in *qmi*) case " $detected_protocols " in *" qmi "*) ;; *) detected_protocols="${detected_protocols}${detected_protocols:+ }qmi" ;; esac ;; esac
    case "$modem_description" in *mbim*) case " $detected_protocols " in *" mbim "*) ;; *) detected_protocols="${detected_protocols}${detected_protocols:+ }mbim" ;; esac ;; esac
    if [ -z "$detected_protocols" ] && [ -n "$modem_description" ]; then
      printf '%s\n' "at"
      return 0
    fi
  fi

  # No modem may be attached during first installation. Install both helpers so
  # a later QMI or MBIM device works without rerunning the installer.
  printf '%s\n' "${detected_protocols:-qmi mbim}"
}

required_package_list() {
  packages=""
  case "$SIMADMIN_DEPS_MODE" in
    minimal)
      packages="ca-certificates curl dbus modemmanager tar udev"
      if truthy "$SIMADMIN_ENABLE_NETWORKMANAGER"; then
        packages="$packages network-manager"
      fi
      ;;
    auto|full)
      packages="ca-certificates curl dbus iproute2 modemmanager tar udev unzip iputils-ping psmisc"
      if truthy "$SIMADMIN_ENABLE_NETWORKMANAGER" || [ "$SIMADMIN_DEPS_MODE" = "full" ]; then
        packages="$packages network-manager"
      fi
      protocols="$(detect_modem_protocols)"
      case " $protocols " in *" qmi "*) packages="$packages libqmi-utils" ;; esac
      case " $protocols " in *" mbim "*) packages="$packages libmbim-utils" ;; esac
      if [ "$SIMADMIN_DEPS_MODE" = "full" ]; then
        packages="$packages iptables"
      fi
      if truthy "$SIMADMIN_INSTALL_LPAC" && detect_lpac_arch >/dev/null 2>&1; then
        packages="$packages libpcsclite1"
      fi
      ;;
    skip) packages="" ;;
  esac
  printf '%s\n' "$packages"
}

install_system_dependencies() {
  if ! truthy "$SIMADMIN_INSTALL_SYSTEM_DEPS"; then
    SIMADMIN_DEPS_MODE=skip
  fi
  if [ "$SIMADMIN_DEPS_MODE" = "skip" ]; then
    echo "==> skipping system dependency installation"
    return 0
  fi

  allow_optional=0
  if [ "$SIMADMIN_DEPS_MODE" = "auto" ]; then
    allow_optional=1
  fi

  required_packages="$(required_package_list)"
  ensure_package_list "runtime dependencies" "$required_packages" "$allow_optional"
}

install_bootstrap_dependencies() {
  if ! truthy "$SIMADMIN_INSTALL_SYSTEM_DEPS" || [ "$SIMADMIN_DEPS_MODE" = "skip" ]; then
    require_cmd curl
    require_cmd tar
    return 0
  fi
  ensure_package_list "installer dependencies" "ca-certificates curl tar"
}

remove_legacy_networkmanager_modem_unmanaged() {
  nm_conf="/etc/NetworkManager/conf.d/99-simadmin-unmanaged-modem.conf"
  LEGACY_NM_CONFIG_REMOVED=0
  if [ -f "$nm_conf" ]; then
    echo "==> removing legacy NetworkManager wwan unmanaged configuration"
    rm -f "$nm_conf"
    LEGACY_NM_CONFIG_REMOVED=1
  fi
}

start_runtime_services() {
  if ! systemctl is-enabled --quiet ModemManager.service 2>/dev/null; then
    echo "==> enabling ModemManager"
    systemctl enable ModemManager.service >/dev/null
  fi
  if ! systemctl is-active --quiet ModemManager.service; then
    echo "==> starting ModemManager"
    systemctl start ModemManager.service
  fi

  if truthy "$SIMADMIN_ENABLE_NETWORKMANAGER"; then
    if ! systemctl is-enabled --quiet NetworkManager.service 2>/dev/null; then
      echo "==> enabling NetworkManager"
      systemctl enable NetworkManager.service >/dev/null
    fi
    if systemctl is-active --quiet NetworkManager.service; then
      if [ "${LEGACY_NM_CONFIG_REMOVED:-0}" -eq 1 ]; then
        echo "==> restarting NetworkManager after removing legacy configuration"
        systemctl restart NetworkManager.service
      fi
    else
      echo "==> starting NetworkManager"
      systemctl start NetworkManager.service
    fi
  else
    echo "==> leaving NetworkManager state unchanged (SIMADMIN_ENABLE_NETWORKMANAGER=${SIMADMIN_ENABLE_NETWORKMANAGER})"
    echo "    nmcli-dependent WLAN and cellular data features are not guaranteed by this installation"
  fi
}

refresh_modem_devices() {
  should_refresh=0
  if truthy "$SIMADMIN_REFRESH_MODEM_DEVICES"; then
    should_refresh=1
  elif [ "$SIMADMIN_REFRESH_MODEM_DEVICES" = "auto" ]; then
    if [ "${FIRST_INSTALL:-0}" -eq 1 ] || [ "${SYSTEM_DEPS_CHANGED:-0}" -eq 1 ]; then
      should_refresh=1
    elif command -v mmcli >/dev/null 2>&1 && ! mmcli -L 2>/dev/null | grep -q '/Modem/'; then
      should_refresh=1
    fi
  fi
  if [ "$should_refresh" -ne 1 ]; then
    echo "==> skipping modem udev refresh (SIMADMIN_REFRESH_MODEM_DEVICES=${SIMADMIN_REFRESH_MODEM_DEVICES})"
    return 0
  fi

  if command -v udevadm >/dev/null 2>&1; then
    echo "==> reloading udev rules and refreshing modem candidates"
    udevadm control --reload-rules
    for subsystem in usb tty usbmisc net; do
      udevadm trigger --action=change --subsystem-match="$subsystem" || true
    done
    udevadm settle --timeout=15 || true
  else
    echo "warning: udevadm is unavailable; reconnect the modem after installation" >&2
  fi

  systemctl restart ModemManager.service
}

download_with_proxies() {
  src_url="$1"
  dst_path="$2"

  case "$src_url" in
    https://github.com/*|https://raw.githubusercontent.com/*|https://objects.githubusercontent.com/*|https://api.github.com/*)
      for proxy in $GH_PROXY $GH_PROXY_FALLBACKS ""; do
        url="${proxy}${src_url}"
        echo "    ${url}"
        if curl -fsSL --connect-timeout 15 --max-time 300 --retry 2 --retry-delay 2 "$url" -o "$dst_path"; then
          return 0
        fi
        echo "    download failed, trying next mirror" >&2
      done
      ;;
    *)
      echo "    ${src_url}"
      curl -fsSL --connect-timeout 15 --max-time 300 --retry 2 --retry-delay 2 "$src_url" -o "$dst_path"
      return $?
      ;;
  esac

  return 1
}

read_with_proxies() {
  src_url="$1"

  case "$src_url" in
    https://github.com/*|https://raw.githubusercontent.com/*|https://objects.githubusercontent.com/*|https://api.github.com/*)
      for proxy in $GH_PROXY $GH_PROXY_FALLBACKS ""; do
        url="${proxy}${src_url}"
        echo "    ${url}" >&2
        if curl -fsSL --connect-timeout 15 --max-time 60 --retry 2 --retry-delay 2 "$url"; then
          return 0
        fi
        echo "    download failed, trying next mirror" >&2
      done
      ;;
    *)
      echo "    ${src_url}" >&2
      curl -fsSL --connect-timeout 15 --max-time 60 --retry 2 --retry-delay 2 "$src_url"
      return $?
      ;;
  esac

  return 1
}

release_api_url() {
  digest_version="${DOWNLOADED_RELEASE_VERSION:-$VERSION}"
  if [ "$digest_version" = "latest" ]; then
    printf 'https://api.github.com/repos/%s/releases/latest\n' "$REPO"
  else
    printf 'https://api.github.com/repos/%s/releases/tags/%s\n' "$REPO" "$(version_to_tag "$digest_version")"
  fi
}

release_asset_digest() {
  digest_asset_name="$1"
  awk -v expected="$digest_asset_name" '
    /"name"[[:space:]]*:/ {
      value = $0
      sub(/^.*"name"[[:space:]]*:[[:space:]]*"/, "", value)
      sub(/".*$/, "", value)
      norm_val = value
      sub(/-v[0-9]+\.[0-9]+(\.[0-9]+)?(-[a-zA-Z0-9.]+)?\.tar\.gz$/, ".tar.gz", norm_val)
      norm_exp = expected
      sub(/-v[0-9]+\.[0-9]+(\.[0-9]+)?(-[a-zA-Z0-9.]+)?\.tar\.gz$/, ".tar.gz", norm_exp)
      selected = (value == expected || norm_val == norm_exp)
    }
    selected && /"digest"[[:space:]]*:/ {
      value = $0
      sub(/^.*"digest"[[:space:]]*:[[:space:]]*"sha256:/, "", value)
      sub(/".*$/, "", value)
      if (value ~ /^[0-9a-fA-F]+$/ && length(value) == 64) {
        print tolower(value)
        exit
      }
    }
  '
}

sha256_file() {
  checksum_path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$checksum_path" | awk '{print tolower($1)}'
  elif command -v busybox >/dev/null 2>&1; then
    busybox sha256sum "$checksum_path" | awk '{print tolower($1)}'
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$checksum_path" | sed 's/^.*= //' | tr '[:upper:]' '[:lower:]'
  else
    echo "error: sha256sum, busybox, or openssl is required for package verification" >&2
    return 1
  fi
}

verify_release_asset() {
  verify_archive="$1"
  verify_asset_name="$2"
  expected_sha="$(printf '%s' "$SIMADMIN_ASSET_SHA256" | tr '[:upper:]' '[:lower:]')"

  case "$SIMADMIN_VERIFY_ASSET" in
    0|false|FALSE|no|NO|off|OFF)
      echo "warning: release asset checksum verification is disabled" >&2
      return 0
      ;;
    auto|1|true|TRUE|yes|YES|on|ON) ;;
    *)
      echo "error: invalid SIMADMIN_VERIFY_ASSET: ${SIMADMIN_VERIFY_ASSET}" >&2
      return 1
      ;;
  esac

  if [ -z "$expected_sha" ] && [ -z "$ASSET_URL" ]; then
    echo "==> reading release asset digest"
    if release_json="$(read_with_proxies "$(release_api_url)")"; then
      expected_sha="$(printf '%s\n' "$release_json" | release_asset_digest "$verify_asset_name")"
    fi
    if [ -z "$expected_sha" ]; then
      latest_api="https://api.github.com/repos/${REPO}/releases/latest"
      if [ "$(release_api_url)" != "$latest_api" ]; then
        if release_json="$(read_with_proxies "$latest_api" 2>/dev/null)"; then
          expected_sha="$(printf '%s\n' "$release_json" | release_asset_digest "$verify_asset_name")"
        fi
      fi
    fi
  fi

  if [ -z "$expected_sha" ]; then
    if [ "$SIMADMIN_VERIFY_ASSET" = "auto" ] && [ -n "$ASSET_URL" ]; then
      echo "warning: custom ASSET_URL has no SIMADMIN_ASSET_SHA256; authenticity was not verified" >&2
      return 0
    fi
    echo "error: no trusted SHA-256 digest found for ${verify_asset_name}" >&2
    return 1
  fi
  case "$expected_sha" in *[!0-9a-f]*)
    echo "error: invalid SHA-256 digest for ${verify_asset_name}" >&2
    return 1
  esac
  if [ "${#expected_sha}" -ne 64 ]; then
    echo "error: invalid SHA-256 digest length for ${verify_asset_name}" >&2
    return 1
  fi

  actual_sha="$(sha256_file "$verify_archive")"
  if [ "$actual_sha" != "$expected_sha" ]; then
    echo "error: SHA-256 mismatch for ${verify_asset_name}" >&2
    echo "       expected: ${expected_sha}" >&2
    echo "       actual:   ${actual_sha}" >&2
    return 1
  fi
  echo "==> release asset SHA-256 verified"
}

resolve_source_ref() {
  : "$1"
  if [ "$VERSION" = "latest" ]; then
    source_ref="main"
  else
    source_ref="$(version_to_tag "$VERSION")"
  fi
  case "$source_ref" in ""|*..*|*[!abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_./-]*)
    echo "error: invalid source ref: ${source_ref}" >&2
    return 1
  esac
  printf '%s\n' "$source_ref"
}

resolve_source_urls() {
  package_version="$1"
  selected_source_ref="$(resolve_source_ref "$package_version")"
  [ -n "$SERVICE_URL" ] || SERVICE_URL="${RAW_BASE}/${selected_source_ref}/scripts/system/simadmin.service"
  [ -n "$MODEM_RECOVERY_SCRIPT_URL" ] || MODEM_RECOVERY_SCRIPT_URL="${RAW_BASE}/${selected_source_ref}/scripts/system/simadmin-modem-recovery.sh"
  [ -n "$MODEM_RECOVERY_SERVICE_URL" ] || MODEM_RECOVERY_SERVICE_URL="${RAW_BASE}/${selected_source_ref}/scripts/system/simadmin-modem-recovery.service"
  echo "==> service sources: ${selected_source_ref}"
}

version_to_tag() {
  case "$1" in
    v*) printf '%s\n' "$1" ;;
    *) printf 'v%s\n' "$1" ;;
  esac
}

asset_url_from_tag() {
  tag="$1"
  simadmin_asset_name="$(resolve_simadmin_asset_name "$tag")"
  printf 'https://github.com/%s/releases/download/%s/%s\n' "$REPO" "$tag" "$simadmin_asset_name"
}

normalize_simadmin_arch() {
  case "$1" in
    aarch64|arm64|aarch64-unknown-linux-musl)
      printf '%s\n' "aarch64"
      ;;
    armv7|armv7l|armhf|arm|armv7-unknown-linux-musleabihf)
      printf '%s\n' "armv7"
      ;;
    x86_64|amd64|x86_64-unknown-linux-musl)
      printf '%s\n' "x86_64"
      ;;
    *)
      return 1
      ;;
  esac
}

detect_simadmin_arch() {
  if [ -n "$SIMADMIN_TARGET_ARCH" ]; then
    normalize_simadmin_arch "$SIMADMIN_TARGET_ARCH"
    return $?
  fi

  normalize_simadmin_arch "$(uname -m)"
}

resolve_simadmin_asset_name() {
  if [ -n "$ASSET_NAME" ]; then
    printf '%s\n' "$ASSET_NAME"
    return 0
  fi

  simadmin_arch="$(detect_simadmin_arch)" || {
    echo "error: unsupported architecture: $(uname -m)" >&2
    return 1
  }

  tag_suffix=""
  if [ -n "${1:-}" ] && [ "$1" != "latest" ]; then
    tag_suffix="-$(version_to_tag "$1")"
  fi

  case "${VARIANT:-}" in
    volte)
      printf 'simadmin-volte-%s%s.tar.gz\n' "$simadmin_arch" "$tag_suffix"
      ;;
    vowifi|wfc)
      printf 'simadmin-vowifi-%s%s.tar.gz\n' "$simadmin_arch" "$tag_suffix"
      ;;
    full)
      printf 'simadmin-full-%s%s.tar.gz\n' "$simadmin_arch" "$tag_suffix"
      ;;
    *)
      if truthy "$WFC"; then
        printf 'simadmin-vowifi-%s%s.tar.gz\n' "$simadmin_arch" "$tag_suffix"
      else
        printf 'simadmin-%s%s.tar.gz\n' "$simadmin_arch" "$tag_suffix"
      fi
      ;;
  esac
}

repo_version() {
  version_text="$(read_with_proxies "${RAW_BASE}/main/VERSION" | tr -d '[:space:]')"
  if [ -z "$version_text" ]; then
    return 1
  fi
  printf '%s\n' "$version_text"
}

resolve_latest_tag() {
  target_url="https://github.com/${REPO}/releases/latest"
  for proxy in $GH_PROXY $GH_PROXY_FALLBACKS ""; do
    url="${proxy}${target_url}"
    final_url="$(curl -fsSL --connect-timeout 15 --max-time 30 -o /dev/null -w '%{url_effective}' "$url" 2>/dev/null || true)"
    case "$final_url" in
      */tag/*)
        tag="${final_url##*/tag/}"
        tag="${tag%%\?*}"
        tag="${tag%%/*}"
        tag="$(printf '%s\n' "$tag" | tr -d '[:space:]')"
        if [ -n "$tag" ]; then
          printf '%s\n' "$tag"
          return 0
        fi
        ;;
      */releases/*)
        tag="${final_url##*/}"
        tag="${tag%%\?*}"
        tag="$(printf '%s\n' "$tag" | tr -d '[:space:]')"
        if [ -n "$tag" ] && [ "$tag" != "latest" ]; then
          printf '%s\n' "$tag"
          return 0
        fi
        ;;
    esac
  done

  if fallback_ver="$(repo_version)"; then
    version_to_tag "$fallback_ver"
    return 0
  fi

  return 1
}

resolve_target_tag() {
  if [ -n "${TARGET_TAG:-}" ]; then
    printf '%s\n' "$TARGET_TAG"
    return 0
  fi

  if [ "$VERSION" = "latest" ]; then
    tag="$(resolve_latest_tag || true)"
  else
    tag="$(version_to_tag "$VERSION")"
  fi

  if [ -n "$tag" ]; then
    printf '%s\n' "$tag"
    return 0
  fi

  return 1
}

resolve_asset_url() {
  if [ -n "$ASSET_URL" ]; then
    printf '%s\n' "$ASSET_URL"
    return 0
  fi

  tag="${TARGET_TAG:-}"
  if [ -z "$tag" ]; then
    tag="$(resolve_target_tag || true)"
    TARGET_TAG="$tag"
  fi

  if [ -n "$tag" ]; then
    PRIMARY_RELEASE_VERSION="${tag#v}"
    asset_url_from_tag "$tag"
  else
    printf 'https://github.com/%s/releases/latest/download/%s\n' "$REPO" "$(resolve_simadmin_asset_name "")"
  fi
}

fallback_asset_url() {
  FALLBACK_ASSET_URL=""
  FALLBACK_RELEASE_VERSION=""

  if [ -n "$ASSET_URL" ]; then
    return 1
  fi

  tag="${TARGET_TAG:-}"
  if [ -z "$tag" ]; then
    if [ "$VERSION" = "latest" ]; then
      if FALLBACK_RELEASE_VERSION="$(repo_version)"; then
        tag="$(version_to_tag "$FALLBACK_RELEASE_VERSION")"
      fi
    else
      tag="$(version_to_tag "$VERSION")"
      FALLBACK_RELEASE_VERSION="${tag#v}"
    fi
  else
    FALLBACK_RELEASE_VERSION="${tag#v}"
  fi

  if [ -n "$tag" ]; then
    unversioned_name="$(resolve_simadmin_asset_name "")"
    candidate="https://github.com/${REPO}/releases/download/${tag}/${unversioned_name}"
    if [ "$candidate" != "${PRIMARY_DOWNLOAD_URL:-}" ]; then
      FALLBACK_ASSET_URL="$candidate"
      return 0
    fi
  fi

  unversioned_name="$(resolve_simadmin_asset_name "")"
  candidate="https://github.com/${REPO}/releases/latest/download/${unversioned_name}"
  if [ "$candidate" != "${PRIMARY_DOWNLOAD_URL:-}" ]; then
    FALLBACK_ASSET_URL="$candidate"
    return 0
  fi

  return 1
}

download_release_asset() {
  archive_path="$1"
  primary_url="$2"
  fallback_url=""
  PRIMARY_DOWNLOAD_URL="$primary_url"
  DOWNLOADED_ASSET_URL=""

  echo "==> downloading release asset"
  if download_with_proxies "$primary_url" "$archive_path"; then
    DOWNLOADED_RELEASE_VERSION="${PRIMARY_RELEASE_VERSION:-$VERSION}"
    DOWNLOADED_ASSET_URL="$primary_url"
    return 0
  fi

  if fallback_asset_url && [ "$FALLBACK_ASSET_URL" != "$primary_url" ]; then
    fallback_url="$FALLBACK_ASSET_URL"
    echo "==> primary asset download failed, trying fallback asset: $fallback_url"
    if download_with_proxies "$fallback_url" "$archive_path"; then
      DOWNLOADED_RELEASE_VERSION="${FALLBACK_RELEASE_VERSION:-$VERSION}"
      DOWNLOADED_ASSET_URL="$fallback_url"
      return 0
    fi
  fi

  if [ "${VARIANT:-}" = "vowifi" ] || [ "${VARIANT:-}" = "wfc" ] || truthy "$WFC"; then
    tag="${TARGET_TAG:-}"
    simadmin_arch="$(detect_simadmin_arch || uname -m)"
    if [ -n "$tag" ]; then
      legacy_wfc_url="https://github.com/${REPO}/releases/download/${tag}/simadmin-wfc-${simadmin_arch}.tar.gz"
    else
      legacy_wfc_url="https://github.com/${REPO}/releases/latest/download/simadmin-wfc-${simadmin_arch}.tar.gz"
    fi
    if [ "$legacy_wfc_url" != "$primary_url" ] && [ "$legacy_wfc_url" != "$fallback_url" ]; then
      echo "==> primary asset download failed, trying legacy WFC asset: $legacy_wfc_url"
      if download_with_proxies "$legacy_wfc_url" "$archive_path"; then
        DOWNLOADED_RELEASE_VERSION="${FALLBACK_RELEASE_VERSION:-$VERSION}"
        DOWNLOADED_ASSET_URL="$legacy_wfc_url"
        return 0
      fi
    fi
  fi

  echo "error: failed to download OTA asset" >&2
  echo "       tried: $primary_url" >&2
  if [ -n "$fallback_url" ]; then
    echo "       tried: $fallback_url" >&2
  fi
  exit 1
}

download_managed_source_file() {
  target_file="$1"
  pattern="$2"
  shift 2
  for candidate_url in "$@"; do
    [ -n "$candidate_url" ] || continue
    if download_with_proxies "$candidate_url" "$target_file" 2>/dev/null && grep -q "$pattern" "$target_file" 2>/dev/null; then
      return 0
    fi
  done
  return 1
}

prepare_managed_service_files() {
  downloaded_service="${tmp_dir}/simadmin.service.source"
  downloaded_recovery_script="${tmp_dir}/simadmin-modem-recovery.sh.source"
  downloaded_recovery_service="${tmp_dir}/simadmin-modem-recovery.service.source"
  staged_service="${tmp_dir}/simadmin.service"
  staged_recovery_script="${tmp_dir}/simadmin-modem-recovery.sh"
  staged_recovery_service="${tmp_dir}/simadmin-modem-recovery.service"

  echo "==> downloading systemd and recovery sources"
  if ! download_managed_source_file "$downloaded_service" '^\[Service\]' \
    "$SERVICE_URL" \
    "${RAW_BASE}/${selected_source_ref}/scripts/system/simadmin.service" \
    "${RAW_BASE}/${selected_source_ref}/scripts/simadmin.service" \
    "${RAW_BASE}/main/scripts/system/simadmin.service"; then
    echo "error: invalid SimAdmin systemd service source" >&2
    return 1
  fi

  grep -q '^ExecStart=' "$downloaded_service" || {
    echo "error: SimAdmin systemd service has no ExecStart" >&2
    return 1
  }
  grep -q '^WorkingDirectory=' "$downloaded_service" || {
    echo "error: SimAdmin systemd service has no WorkingDirectory" >&2
    return 1
  }

  if ! download_managed_source_file "$downloaded_recovery_script" '^#!' \
    "$MODEM_RECOVERY_SCRIPT_URL" \
    "${RAW_BASE}/${selected_source_ref}/scripts/system/simadmin-modem-recovery.sh" \
    "${RAW_BASE}/${selected_source_ref}/scripts/simadmin-modem-recovery.sh" \
    "${RAW_BASE}/main/scripts/system/simadmin-modem-recovery.sh"; then
    echo "error: invalid modem recovery script source" >&2
    return 1
  fi

  if ! download_managed_source_file "$downloaded_recovery_service" '^\[Service\]' \
    "$MODEM_RECOVERY_SERVICE_URL" \
    "${RAW_BASE}/${selected_source_ref}/scripts/system/simadmin-modem-recovery.service" \
    "${RAW_BASE}/${selected_source_ref}/scripts/simadmin-modem-recovery.service" \
    "${RAW_BASE}/main/scripts/system/simadmin-modem-recovery.service"; then
    echo "error: invalid modem recovery systemd service source" >&2
    return 1
  fi

  grep -q '^ExecStart=' "$downloaded_recovery_service" || {
    echo "error: modem recovery systemd service has no ExecStart" >&2
    return 1
  }

  sed \
    -e "s#^WorkingDirectory=.*#WorkingDirectory=${INSTALL_DIR}#" \
    -e "s#^ExecStart=.*#ExecStart=${INSTALL_DIR}/simadmin#" \
    "$downloaded_service" > "$staged_service"
  cp "$downloaded_recovery_script" "$staged_recovery_script"
  sed \
    -e "s#^ExecStart=.*#ExecStart=${MODEM_RECOVERY_BIN_DIR}/simadmin-modem-recovery.sh#" \
    "$downloaded_recovery_service" > "$staged_recovery_service"
  chmod 0644 "$staged_service" "$staged_recovery_service"
  chmod 0755 "$staged_recovery_script"
}

validate_release_archive() {
  archive_to_validate="$1"
  if ! tar -tzf "$archive_to_validate" >/dev/null 2>&1; then
    echo "error: release asset is not a valid tar.gz archive" >&2
    return 1
  fi

  if ! tar -tzf "$archive_to_validate" | while IFS= read -r archive_member; do
    while [ "${archive_member#./}" != "$archive_member" ]; do
      archive_member="${archive_member#./}"
    done
    case "$archive_member" in
      ""|/*|..|../*|*/../*|*/..) exit 1 ;;
      simadmin|meta.json|www|www/*) ;;
      *) exit 1 ;;
    esac
  done; then
    echo "error: release archive contains an unsafe or unexpected path" >&2
    return 1
  fi

  if tar -tvzf "$archive_to_validate" | awk 'substr($1, 1, 1) !~ /^[-d]$/ { found=1 } END { exit !found }'; then
    echo "error: release archive contains links or special files" >&2
    return 1
  fi
}

verify_binary_architecture() {
  binary_path="$1"
  expected_binary_arch="$2"
  if truthy "$SIMADMIN_SKIP_ELF_CHECK"; then
    echo "warning: ELF architecture verification is disabled" >&2
    return 0
  fi
  require_cmd od
  # ELF e_machine is at bytes 18-19. Official Linux targets are little-endian.
  set -- $(od -An -tx1 -N20 "$binary_path")
  if [ "$#" -lt 20 ] || [ "$1 $2 $3 $4" != "7f 45 4c 46" ] || [ "$6" != "01" ]; then
    echo "error: package binary is not a supported little-endian ELF file" >&2
    return 1
  fi
  case "$expected_binary_arch" in
    aarch64-unknown-linux-musl)
      [ "$5" = "02" ] && [ "${19} ${20}" = "b7 00" ] || {
        echo "error: package binary is not AArch64 ELF64" >&2
        return 1
      }
      ;;
    x86_64-unknown-linux-musl)
      [ "$5" = "02" ] && [ "${19} ${20}" = "3e 00" ] || {
        echo "error: package binary is not x86_64 ELF64" >&2
        return 1
      }
      ;;
    armv7-unknown-linux-musleabihf)
      [ "$5" = "01" ] && [ "${19} ${20}" = "28 00" ] || {
        echo "error: package binary is not ARMv7 ELF32" >&2
        return 1
      }
      set -- $(od -An -tx1 -j36 -N4 "$binary_path")
      [ "$#" -eq 4 ] && [ $((0x$2 & 4)) -eq 4 ] || {
        echo "error: ARMv7 package does not declare the hard-float ABI" >&2
        return 1
      }
      ;;
    *)
      echo "error: no ELF verifier for architecture ${expected_binary_arch}" >&2
      return 1
      ;;
  esac
}

extract_release_package() {
  archive_to_extract="$1"
  package_dir="${tmp_dir}/pkg"
  validate_release_archive "$archive_to_extract"
  echo "==> extracting package"
  mkdir -p "$package_dir"
  tar --no-same-owner --no-same-permissions -xzf "$archive_to_extract" -C "$package_dir"
  if find "$package_dir" -type l -print -quit | grep -q .; then
    echo "error: release package must not contain symbolic links" >&2
    return 1
  fi
  [ -f "${package_dir}/simadmin" ] || {
    echo "error: invalid package, missing simadmin binary" >&2
    return 1
  }
  [ -d "${package_dir}/www" ] || {
    echo "error: invalid package, missing frontend www directory" >&2
    return 1
  }
}

normalize_lpac_arch() {
  case "$1" in
    aarch64|arm64)
      printf '%s\n' "aarch64"
      ;;
    x86_64|amd64)
      printf '%s\n' "x86_64"
      ;;
    *)
      return 1
      ;;
  esac
}

detect_lpac_arch() {
  # ARMv7 is deliberately excluded from the MVP lpac runtime. Check the
  # running kernel before honoring an override so an explicit ARM64 override
  # cannot install an incompatible lpac binary on a 32-bit device.
  machine_arch="$(uname -m)"
  case "$machine_arch" in
    armv7l|armhf|armv7|arm)
      return 1
      ;;
  esac

  if [ -n "$LPAC_TARGET_ARCH" ]; then
    normalize_lpac_arch "$LPAC_TARGET_ARCH"
    return $?
  fi

  normalize_lpac_arch "$(uname -m)"
}

detect_glibc_version() {
  if command -v getconf >/dev/null 2>&1; then
    version="$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}' || true)"
    if [ -n "$version" ]; then
      printf '%s\n' "$version"
      return 0
    fi
  fi

  if command -v ldd >/dev/null 2>&1; then
    version="$(ldd --version 2>/dev/null | head -n 1 | sed -E 's/.* ([0-9]+\.[0-9]+).*/\1/' || true)"
    if [ -n "$version" ]; then
      printf '%s\n' "$version"
      return 0
    fi
  fi

  printf '%s\n' ""
}

version_le() {
  [ "$1" = "$2" ] && return 0
  [ -n "$1" ] || return 0
  [ -n "$2" ] || return 1
  first="$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n 1)"
  [ "$first" = "$1" ]
}

normalize_version_value() {
  value="$1"
  value="${value#refs/tags/}"
  value="${value#tags/}"
  value="${value#v}"
  value="${value#V}"
  printf '%s\n' "$value"
}

version_lt() {
  left="$(normalize_version_value "$1")"
  right="$(normalize_version_value "$2")"
  [ -n "$left" ] || return 0
  [ -n "$right" ] || return 1
  [ "$left" = "$right" ] && return 1
  version_le "$left" "$right"
}

version_token_from_text() {
  printf '%s\n' "$1" \
    | tr '",:{}[]()' '          ' \
    | tr '[:space:]' '\n' \
    | sed -nE '/^[vV]?[0-9]+(\.[0-9]+)+([-+][0-9A-Za-z._-]+)?$/p' \
    | head -n 1
}

json_string_field() {
  field="$1"
  sed -nE 's/.*"'"$field"'"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' | head -n 1
}

resolve_lpac_asset_name() {
  arch="$1"

  if [ -n "$LPAC_ASSET_NAME" ]; then
    printf '%s\n' "$LPAC_ASSET_NAME"
    return 0
  fi

  case "$LPAC_ASSET_FLAVOR" in
    compat)
      glibc_version="$(detect_glibc_version)"
      resolve_lpac_compat_asset_name "$arch" "$glibc_version"
      ;;
    ""|default)
      printf 'lpac-linux-%s.zip\n' "$arch"
      ;;
    with-qmi)
      printf 'lpac-linux-%s-with-qmi.zip\n' "$arch"
      ;;
    without-lto)
      printf 'lpac-linux-%s-without-lto.zip\n' "$arch"
      ;;
    *)
      echo "warning: unsupported LPAC_ASSET_FLAVOR=${LPAC_ASSET_FLAVOR}, skipping lpac install" >&2
      return 1
      ;;
  esac
}

resolve_lpac_compat_asset_name() {
  arch="$1"
  glibc_version="$2"

  if { [ "$arch" = "aarch64" ] || [ "$arch" = "x86_64" ]; } \
    && version_le "2.31" "$glibc_version"; then
    printf 'lpac-linux-%s-glibc2.31.zip\n' "$arch"
  else
    printf 'lpac-linux-%s-with-qmi.zip\n' "$arch"
  fi
}

resolve_lpac_asset_url() {
  if [ -n "$LPAC_ASSET_URL" ]; then
    printf '%s\n' "$LPAC_ASSET_URL"
    return 0
  fi

  arch="$(detect_lpac_arch)" || return 1
  asset_name="$(resolve_lpac_asset_name "$arch")" || return 1
  if [ "$LPAC_ASSET_FLAVOR" = "compat" ]; then
    case "$asset_name" in
      lpac-linux-aarch64-glibc2.31.zip|lpac-linux-x86_64-glibc2.31.zip)
        printf '%s/%s\n' "$LPAC_COMPAT_RELEASE_BASE_URL" "$asset_name"
        return 0
        ;;
    esac
  fi
  printf '%s/%s\n' "$LPAC_RELEASE_BASE_URL" "$asset_name"
}

extract_lpac_archive() {
  archive="$1"
  target="$2"

  mkdir -p "$target"
  if command -v unzip >/dev/null 2>&1; then
    unzip -oq "$archive" -d "$target"
    return $?
  fi

  if command -v busybox >/dev/null 2>&1; then
    busybox unzip -oq "$archive" -d "$target"
    return $?
  fi

  if command -v python3 >/dev/null 2>&1; then
    python3 - "$archive" "$target" <<'PY'
import sys
from zipfile import ZipFile

archive, target = sys.argv[1], sys.argv[2]
ZipFile(archive).extractall(target)
PY
    return $?
  fi

  # Use the installed SimAdmin-compatible binary when external tools are unavailable.
  zip_extractor="${SIMADMIN_ZIP_EXTRACTOR:-${INSTALL_DIR}/simadmin}"
  if [ -x "$zip_extractor" ]; then
    echo "    using ${zip_extractor} extract-zip (built-in)"
    "$zip_extractor" extract-zip "$archive" "$target"
    return $?
  fi

  echo "warning: no zip extractor available, skipping lpac install" >&2
  return 1
}

copy_lpac_tree() {
  copy_extract_dir="$1"
  copy_destination="$2"
  copy_asset_url="$3"

  if [ -f "${copy_extract_dir}/lpac" ]; then
    copy_bundle_root="${copy_extract_dir}"
  elif [ -f "${copy_extract_dir}/executables/lpac" ]; then
    copy_bundle_root="${copy_extract_dir}/executables"
  else
    copy_bundle_root="$(find "$copy_extract_dir" -type f -name lpac -exec dirname {} \; | head -n 1 || true)"
  fi

  if [ -z "$copy_bundle_root" ] || [ ! -f "${copy_bundle_root}/lpac" ]; then
    echo "warning: downloaded lpac asset does not contain lpac executable" >&2
    return 1
  fi

  rm -rf "${copy_destination}"
  mkdir -p "${copy_destination}"
  cp -R "${copy_bundle_root}/." "${copy_destination}/"

  if [ -d "${copy_extract_dir}/lib" ] && [ ! -d "${copy_destination}/lib" ]; then
    mkdir -p "${copy_destination}/lib"
    cp -R "${copy_extract_dir}/lib/." "${copy_destination}/lib/"
  fi

  if [ -d "${copy_extract_dir}/libraries" ] && [ ! -d "${copy_destination}/lib" ]; then
    mkdir -p "${copy_destination}/lib"
    cp -R "${copy_extract_dir}/libraries/." "${copy_destination}/lib/"
  fi

  normalize_lpac_library_links "${copy_destination}/lib" "libqmi-glib"
  normalize_lpac_library_links "${copy_destination}/lib" "libmbim-glib"

  chmod -R a+rX "${copy_destination}"
  chmod 0755 "${copy_destination}/lpac"

  cat > "${copy_destination}/SOURCE.txt" <<EOF
lpac is installed from:
${copy_asset_url}

Project:
https://github.com/estkme-group/lpac
EOF
}

normalize_lpac_library_links() {
  library_dir="$1"
  library_name="$2"
  [ -d "$library_dir" ] || return 0

  real_library="$(find "$library_dir" -type f -name "${library_name}.so.*.*.*" -print | head -n 1 || true)"
  [ -n "$real_library" ] || return 0
  real_name="$(basename "$real_library")"
  soname="$(printf '%s\n' "$real_name" | sed -nE 's/^(.+\.so\.[0-9]+)\..*$/\1/p')"
  [ -n "$soname" ] || return 0

  for alias in "${library_name}.so" "$soname"; do
    [ "$alias" = "$real_name" ] && continue
    rm -f "${library_dir}/${alias}"
    ln -s "$real_name" "${library_dir}/${alias}"
  done
}

lpac_env_prefix() {
  lpac_path="$1"
  lpac_home="$(dirname "$lpac_path")"
  printf '%s\n' "${lpac_home}/lib${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
}

lpac_binary_path_usable() {
  lpac_path="$1"
  if [ ! -x "$lpac_path" ]; then
    return 1
  fi

  if ! output=$(LPAC_APDU=stdio LPAC_HTTP=stdio \
    LD_LIBRARY_PATH="$(lpac_env_prefix "$lpac_path")" \
    "$lpac_path" driver list 2>&1); then
    return 1
  fi
  case "$output" in
    *GLIBC_*|*No\ such\ file\ or\ directory*|*Permission\ denied*|*error\ while\ loading\ shared\ libraries*)
      return 1
      ;;
  esac

  compact_output="$(printf '%s' "$output" | tr -d '[:space:]')"
  if ! printf '%s\n' "$compact_output" | grep -Eq '"LPAC_APDU":\[[^]]*"qmi"'; then
    return 1
  fi
  if ! printf '%s\n' "$compact_output" | grep -Eq '"LPAC_HTTP":\[[^]]*"curl"'; then
    return 1
  fi

  return 0
}

lpac_binary_usable() {
  lpac_home="$1"
  lpac_binary_path_usable "${lpac_home}/lpac"
}

lpac_command_version() {
  lpac_path="$1"
  [ -x "$lpac_path" ] || return 1

  for arg in version --version -v; do
    output="$(LD_LIBRARY_PATH="$(lpac_env_prefix "$lpac_path")" "$lpac_path" "$arg" 2>&1 || true)"
    version="$(version_token_from_text "$output")"
    case "$version" in
      ""|0.0.0*|0.0|0)
        continue
        ;;
    esac
    if [ -n "$version" ]; then
      printf '%s\n' "$version"
      return 0
    fi
  done

  return 1
}

lpac_installed_version() {
  lpac_path="$1"
  lpac_home="$(dirname "$lpac_path")"

  if [ -f "${lpac_home}/VERSION.txt" ]; then
    version="$(version_token_from_text "$(cat "${lpac_home}/VERSION.txt")")"
    if [ -n "$version" ]; then
      printf '%s\n' "$version"
      return 0
    fi
  fi

  if version="$(lpac_command_version "$lpac_path")"; then
    printf '%s\n' "$version"
    return 0
  fi

  if [ -f "${lpac_home}/SOURCE.txt" ]; then
    version="$(version_token_from_text "$(cat "${lpac_home}/SOURCE.txt")")"
    if [ -n "$version" ]; then
      printf '%s\n' "$version"
      return 0
    fi
  fi

  return 1
}

lpac_release_version_from_url() {
  url="$1"
  tag="$(printf '%s\n' "$url" | sed -nE 's#^.*/releases/download/([^/]+)/.*#\1#p' | head -n 1)"
  case "$tag" in
    ""|latest)
      return 1
      ;;
  esac

  version="$(version_token_from_text "$tag")"
  [ -n "$version" ] || return 1
  printf '%s\n' "$version"
}

lpac_asset_name_from_url() {
  url="$1"
  asset_name="${url%%\?*}"
  asset_name="${asset_name##*/}"
  printf '%s\n' "$asset_name"
}

lpac_url_source() {
  url="$1"
  case "$url" in
    "$LPAC_COMPAT_RELEASE_BASE_URL"/*|https://github.com/"$REPO"/releases/download/lpac/*)
      printf '%s\n' "compat"
      ;;
    "$LPAC_RELEASE_BASE_URL"/*|https://github.com/"$LPAC_REPO"/releases/latest/download/*|https://github.com/"$LPAC_REPO"/releases/download/*)
      printf '%s\n' "official"
      ;;
    *)
      printf '%s\n' "custom"
      ;;
  esac
}

compat_lpac_release_version() {
  lpac_url="$1"
  manifest_url="${LPAC_COMPAT_RELEASE_BASE_URL}/${LPAC_COMPAT_MANIFEST_NAME}"
  manifest="$(read_with_proxies "$manifest_url" 2>/dev/null || true)"
  [ -n "$manifest" ] || return 1

  asset_name="$(lpac_asset_name_from_url "$lpac_url")"
  if [ -n "$asset_name" ]; then
    asset_record="$(printf '%s\n' "$manifest" \
      | tr '\n' ' ' \
      | sed 's/}[[:space:]]*,[[:space:]]*{/}\
{/g' \
      | grep "\"name\"[[:space:]]*:[[:space:]]*\"${asset_name}\"" \
      | head -n 1 || true)"
    version="$(printf '%s\n' "$asset_record" | json_string_field version)"
    version="$(version_token_from_text "$version")"
    if [ -n "$version" ]; then
      printf '%s\n' "$version"
      return 0
    fi
  fi

  version="$(printf '%s\n' "$manifest" | json_string_field version)"
  version="$(version_token_from_text "$version")"
  [ -n "$version" ] || return 1
  printf '%s\n' "$version"
}

official_lpac_release_version() {
  lpac_url="$1"

  version="$(lpac_release_version_from_url "$lpac_url" || true)"
  if [ -n "$version" ]; then
    printf '%s\n' "$version"
    return 0
  fi

  json="$(read_with_proxies "$LPAC_LATEST_RELEASE_API_URL" 2>/dev/null || true)"
  tag="$(printf '%s\n' "$json" | json_string_field tag_name)"
  version="$(version_token_from_text "$tag")"
  if [ -n "$version" ]; then
    printf '%s\n' "$version"
    return 0
  fi

  html="$(read_with_proxies "$LPAC_LATEST_RELEASE_URL" 2>/dev/null || true)"
  tag="$(printf '%s\n' "$html" \
    | sed -nE 's#.*releases/(tag|expanded_assets)/([vV]?[0-9]+(\.[0-9]+)+[^"<>/?[:space:]]*).*#\2#p' \
    | head -n 1)"
  version="$(version_token_from_text "$tag")"
  if [ -n "$version" ]; then
    printf '%s\n' "$version"
    return 0
  fi

  return 1
}

resolve_lpac_target_version() {
  lpac_url="$1"

  if [ -n "$LPAC_TARGET_VERSION" ]; then
    version="$(version_token_from_text "$LPAC_TARGET_VERSION")"
    [ -n "$version" ] || return 1
    LPAC_TARGET_RELEASE_SOURCE="override"
    printf '%s\n' "$version"
    return 0
  fi

  LPAC_TARGET_RELEASE_SOURCE="$(lpac_url_source "$lpac_url")"
  case "$LPAC_TARGET_RELEASE_SOURCE" in
    compat)
      compat_lpac_release_version "$lpac_url"
      ;;
    official)
      official_lpac_release_version "$lpac_url"
      ;;
    *)
      for candidate in "$lpac_url" "$LPAC_ASSET_URL" "$LPAC_RELEASE_BASE_URL"; do
        version="$(lpac_release_version_from_url "$candidate" || true)"
        if [ -n "$version" ]; then
          printf '%s\n' "$version"
          return 0
        fi
      done

      LPAC_TARGET_RELEASE_SOURCE="official"
      official_lpac_release_version "$LPAC_RELEASE_BASE_URL"
      ;;
  esac
}

find_current_lpac_path() {
  private_path="${INSTALL_DIR}/lpac/lpac"
  if [ -e "$private_path" ] || [ -d "${INSTALL_DIR}/lpac" ]; then
    printf '%s\n' "$private_path"
    return 0
  fi

  if command_path="$(command -v lpac 2>/dev/null)"; then
    printf '%s\n' "$command_path"
    return 0
  fi

  return 1
}

write_lpac_version_file() {
  lpac_home="$1"
  version="$2"
  [ -n "$version" ] || return 0
  printf '%s\n' "$version" > "${lpac_home}/VERSION.txt"
  chmod 0644 "${lpac_home}/VERSION.txt" || true
}

lpac_installed_compat_revision() {
  lpac_path="$1"
  revision_file="$(dirname "$lpac_path")/COMPAT_REVISION.txt"
  [ -f "$revision_file" ] || return 1

  revision="$(tr -d '[:space:]' < "$revision_file")"
  case "$revision" in
    ''|*[!0-9]*) return 1 ;;
  esac
  printf '%s\n' "$revision"
}

compat_lpac_release_revision() {
  lpac_url="$1"
  manifest_url="${LPAC_COMPAT_RELEASE_BASE_URL}/${LPAC_COMPAT_MANIFEST_NAME}"
  manifest="$(read_with_proxies "$manifest_url" 2>/dev/null || true)"
  [ -n "$manifest" ] || return 1

  asset_name="$(lpac_asset_name_from_url "$lpac_url")"
  [ -n "$asset_name" ] || return 1
  asset_record="$(printf '%s\n' "$manifest" \
    | tr '\n' ' ' \
    | sed 's/}[[:space:]]*,[[:space:]]*{/}\
{/g' \
    | grep "\"name\"[[:space:]]*:[[:space:]]*\"${asset_name}\"" \
    | head -n 1 || true)"
  revision="$(printf '%s\n' "$asset_record" | json_string_field compat_revision)"
  [ -n "$revision" ] || revision="$(printf '%s\n' "$manifest" | json_string_field compat_revision)"
  case "$revision" in
    ''|*[!0-9]*) return 1 ;;
  esac
  printf '%s\n' "$revision"
}

write_lpac_compat_revision_file() {
  lpac_home="$1"
  revision="$2"
  case "$revision" in
    ''|*[!0-9]*) return 0 ;;
  esac
  printf '%s\n' "$revision" > "${lpac_home}/COMPAT_REVISION.txt"
  chmod 0644 "${lpac_home}/COMPAT_REVISION.txt" || true
}

lpac_install_needed() {
  lpac_path="$1"
  lpac_url="$2"
  LPAC_INSTALL_REASON=""
  LPAC_TARGET_RELEASE_VERSION=""
  LPAC_TARGET_RELEASE_SOURCE=""
  LPAC_TARGET_COMPAT_REVISION=""

  if [ -z "$lpac_path" ] || [ ! -x "$lpac_path" ]; then
    LPAC_INSTALL_REASON="not installed"
    return 0
  fi

  if ! lpac_binary_path_usable "$lpac_path"; then
    LPAC_INSTALL_REASON="installed lpac is not usable"
    return 0
  fi

  current_version="$(lpac_installed_version "$lpac_path" || true)"
  if [ -z "$current_version" ]; then
    LPAC_INSTALL_REASON="installed version is unknown"
    return 0
  fi

  LPAC_TARGET_RELEASE_SOURCE="$(lpac_url_source "$lpac_url")"
  LPAC_TARGET_RELEASE_VERSION="$(resolve_lpac_target_version "$lpac_url" || true)"
  if [ -z "$LPAC_TARGET_RELEASE_VERSION" ]; then
    LPAC_INSTALL_REASON="latest version could not be verified"
    return 0
  fi

  if [ "$LPAC_TARGET_RELEASE_SOURCE" = "compat" ]; then
    LPAC_TARGET_COMPAT_REVISION="$(compat_lpac_release_revision "$lpac_url" || true)"
    if [ -n "$LPAC_TARGET_COMPAT_REVISION" ]; then
      installed_compat_revision="$(lpac_installed_compat_revision "$lpac_path" || true)"
      if [ -z "$installed_compat_revision" ] \
        || version_lt "$installed_compat_revision" "$LPAC_TARGET_COMPAT_REVISION"; then
        LPAC_INSTALL_REASON="compatibility bundle revision ${installed_compat_revision:-0} -> ${LPAC_TARGET_COMPAT_REVISION}"
        return 0
      fi
    fi
  fi

  if version_lt "$current_version" "$LPAC_TARGET_RELEASE_VERSION"; then
    LPAC_INSTALL_REASON="installed ${current_version}, ${LPAC_TARGET_RELEASE_SOURCE:-target} ${LPAC_TARGET_RELEASE_VERSION}"
    return 0
  fi

  echo "==> skipping lpac install (installed ${current_version}, ${LPAC_TARGET_RELEASE_SOURCE:-target} ${LPAC_TARGET_RELEASE_VERSION})"
  return 1
}

install_lpac() {
  lpac_dst="${INSTALL_DIR}/lpac"
  lpac_archive="${tmp_dir}/lpac.zip"
  lpac_extract="${tmp_dir}/lpac-extract"
  lpac_stage="${tmp_dir}/lpac-stage"

  if ! truthy "$SIMADMIN_INSTALL_LPAC"; then
    echo "==> skipping lpac install (SIMADMIN_INSTALL_LPAC=${SIMADMIN_INSTALL_LPAC})"
    return 0
  fi

  lpac_arch="$(detect_lpac_arch || true)"
  if [ -z "$lpac_arch" ]; then
    echo "warning: unsupported device arch for lpac: $(uname -m), skipping lpac install" >&2
    return 0
  fi

  lpac_url="$(resolve_lpac_asset_url || true)"
  if [ -z "$lpac_url" ]; then
    echo "warning: failed to resolve lpac asset, skipping lpac install" >&2
    return 0
  fi

  current_lpac_path="$(find_current_lpac_path || true)"
  if ! lpac_install_needed "$current_lpac_path" "$lpac_url"; then
    return 0
  fi

  if [ -z "$LPAC_TARGET_RELEASE_VERSION" ]; then
    LPAC_TARGET_RELEASE_VERSION="$(resolve_lpac_target_version "$lpac_url" || true)"
  fi

  echo "==> installing lpac for ${lpac_arch} (${LPAC_INSTALL_REASON})"
  if ! download_with_proxies "$lpac_url" "$lpac_archive"; then
    echo "warning: failed to download lpac, keeping existing lpac if present" >&2
    return 0
  fi

  if ! extract_lpac_archive "$lpac_archive" "$lpac_extract"; then
    echo "warning: failed to extract lpac, keeping existing lpac if present" >&2
    return 0
  fi

  if copy_lpac_tree "$lpac_extract" "$lpac_stage" "$lpac_url"; then
    detected_version="$(lpac_command_version "${lpac_stage}/lpac" || true)"
    case "$detected_version" in
      ""|0.0.0*|0.0|0)
        detected_version="$LPAC_TARGET_RELEASE_VERSION"
        ;;
    esac
    write_lpac_version_file "$lpac_stage" "$detected_version"
    write_lpac_compat_revision_file "$lpac_stage" "$LPAC_TARGET_COMPAT_REVISION"
    if ! lpac_binary_usable "$lpac_stage"; then
      echo "warning: downloaded lpac does not provide the required qmi/curl drivers or has missing libraries; keeping existing lpac" >&2
      return 0
    fi

    lpac_previous="${lpac_dst}.previous"
    rm -rf "$lpac_previous"
    had_existing=0
    if [ -e "$lpac_dst" ] || [ -L "$lpac_dst" ]; then
      mv "$lpac_dst" "$lpac_previous"
      had_existing=1
    fi
    if ! mv "$lpac_stage" "$lpac_dst"; then
      if [ "$had_existing" -eq 1 ]; then
        mv "$lpac_previous" "$lpac_dst" || true
      fi
      echo "warning: failed to activate lpac, restored previous installation" >&2
      return 0
    fi
    if [ "$had_existing" -eq 1 ]; then
      rm -rf "$lpac_previous"
    fi

    if [ -n "$detected_version" ]; then
      echo "==> lpac ${detected_version} installed to ${lpac_dst}"
    else
      echo "==> lpac installed to ${lpac_dst}"
    fi
  else
    echo "warning: failed to install lpac, keeping existing lpac if present" >&2
  fi
}

expected_target_triple() {
  case "$RUNTIME_ARCH" in
    aarch64) printf '%s\n' "aarch64-unknown-linux-musl" ;;
    armv7) printf '%s\n' "armv7-unknown-linux-musleabihf" ;;
    x86_64) printf '%s\n' "x86_64-unknown-linux-musl" ;;
    *) return 1 ;;
  esac
}

target_edition() {
  case "${VARIANT:-}" in
    volte)
      printf '%s\n' "volte"
      return 0
      ;;
    vowifi)
      printf '%s\n' "vowifi"
      return 0
      ;;
    full)
      printf '%s\n' "full"
      return 0
      ;;
    wfc)
      printf '%s\n' "wfc"
      return 0
      ;;
  esac

  if truthy "$WFC"; then
    printf '%s\n' "wfc"
    return 0
  fi

  case "${ASSET_NAME:-}" in
    *volte*) printf '%s\n' "volte" ;;
    *vowifi*) printf '%s\n' "vowifi" ;;
    *full*) printf '%s\n' "full" ;;
    *wfc*) printf '%s\n' "wfc" ;;
    *) printf '%s\n' "standard" ;;
  esac
}

validate_package_metadata() {
  expected_arch="$1"
  expected_edition="$2"
  package_meta="${package_dir}/meta.json"
  PACKAGE_VERSION=""

  if [ ! -f "$package_meta" ]; then
    if [ "$expected_arch" = "armv7-unknown-linux-musleabihf" ]; then
      echo "error: ARMv7 package has no meta.json; refusing unverified installation" >&2
      return 1
    fi
    echo "warning: legacy package has no meta.json; architecture metadata is unavailable" >&2
    return 0
  fi

  package_arch="$(json_string_field arch < "$package_meta")"
  if [ -z "$package_arch" ]; then
    echo "error: invalid package, meta.json is missing arch" >&2
    return 1
  fi
  if [ "$package_arch" != "$expected_arch" ]; then
    echo "error: package architecture mismatch: expected ${expected_arch}, got ${package_arch}" >&2
    return 1
  fi
  package_edition="$(json_string_field edition < "$package_meta")"
  if [ -n "$package_edition" ]; then
    edition_matched=0
    if [ "$package_edition" = "$expected_edition" ]; then
      edition_matched=1
    elif [ "$expected_edition" = "vowifi" ] && [ "$package_edition" = "wfc" ]; then
      edition_matched=1
    elif [ "$expected_edition" = "wfc" ] && [ "$package_edition" = "vowifi" ]; then
      edition_matched=1
    fi
    if [ "$edition_matched" -ne 1 ]; then
      echo "error: package edition mismatch: expected ${expected_edition}, got ${package_edition}" >&2
      return 1
    fi
  fi
  PACKAGE_VERSION="$(json_string_field version < "$package_meta")"
}

check_install_disk_space() {
  install_parent="${INSTALL_DIR%/*}"
  [ -n "$install_parent" ] || install_parent="/"
  existing_parent="$install_parent"
  while [ ! -d "$existing_parent" ] && [ "$existing_parent" != "/" ]; do
    existing_parent="${existing_parent%/*}"
    [ -n "$existing_parent" ] || existing_parent="/"
  done
  required_kb="$(du -sk "$package_dir" | awk '{print $1 * 3 + 2048}')"
  available_kb="$(df -Pk "$existing_parent" | awk 'NR == 2 {print $4}')"
  case "$required_kb:$available_kb" in *[!0-9:]*) return 0 ;; esac
  if [ "$available_kb" -lt "$required_kb" ]; then
    echo "error: insufficient disk space: need about ${required_kb} KiB, available ${available_kb} KiB" >&2
    return 1
  fi
}

files_equal() {
  left_file="$1"
  right_file="$2"
  [ -f "$left_file" ] && [ -f "$right_file" ] || return 1
  [ ! -L "$left_file" ] && [ ! -L "$right_file" ] || return 1
  [ "$(sha256_file "$left_file")" = "$(sha256_file "$right_file")" ]
}

directory_fingerprint() {
  tree_root="$1"
  fingerprint_manifest="${tmp_dir}/tree.$$.manifest"
  (
    cd "$tree_root"
    find . -type d -print | LC_ALL=C sort | while IFS= read -r relative_dir; do
      printf 'directory  %s\n' "${relative_dir#./}"
    done
    find . -type f -print | LC_ALL=C sort | while IFS= read -r relative_file; do
      relative_path="${relative_file#./}"
      relative_hash="$(sha256_file "$relative_path")"
      printf '%s  %s\n' "$relative_hash" "$relative_path"
    done
  ) > "$fingerprint_manifest"
  sha256_file "$fingerprint_manifest"
}

directories_equal() {
  left_dir="$1"
  right_dir="$2"
  [ -d "$left_dir" ] && [ -d "$right_dir" ] || return 1
  if find "$left_dir" "$right_dir" -type l -print -quit | grep -q .; then
    return 1
  fi
  [ "$(directory_fingerprint "$left_dir")" = "$(directory_fingerprint "$right_dir")" ]
}

prepare_transaction_files() {
  selected_edition="$1"
  mkdir -p "$INSTALL_DIR" "$SYSTEMD_UNIT_DIR" "$MODEM_RECOVERY_BIN_DIR"

  STAGED_BIN="${INSTALL_DIR}/.simadmin.new.$$"
  STAGED_WWW="${INSTALL_DIR}/.www.new.$$"
  STAGED_META="${INSTALL_DIR}/.meta.json.new.$$"
  STAGED_MAIN_UNIT="${SYSTEMD_UNIT_DIR}/.${SERVICE_NAME}.service.new.$$"
  STAGED_RECOVERY_SCRIPT="${MODEM_RECOVERY_BIN_DIR}/.simadmin-modem-recovery.sh.new.$$"
  STAGED_RECOVERY_UNIT="${SYSTEMD_UNIT_DIR}/.simadmin-modem-recovery.service.new.$$"

  cleanup_staged_transaction
  install -m 0755 "${package_dir}/simadmin" "$STAGED_BIN"
  mkdir -p "$STAGED_WWW"
  cp -R "${package_dir}/www/." "$STAGED_WWW/"
  chmod -R a+rX "$STAGED_WWW"
  if [ -f "${package_dir}/meta.json" ]; then
    install -m 0644 "${package_dir}/meta.json" "$STAGED_META"
    if ! grep -q '"edition"' "$STAGED_META"; then
      sed -i "s/}/, \"edition\": \"${selected_edition}\"}/" "$STAGED_META"
    fi
  else
    cat > "$STAGED_META" <<EOF
{
  "version": "${VERSION}",
  "edition": "${selected_edition}"
}
EOF
    chmod 0644 "$STAGED_META"
  fi
  install -m 0644 "$staged_service" "$STAGED_MAIN_UNIT"
  install -m 0755 "$staged_recovery_script" "$STAGED_RECOVERY_SCRIPT"
  install -m 0644 "$staged_recovery_service" "$STAGED_RECOVERY_UNIT"

  APP_CHANGED=1
  if files_equal "$STAGED_BIN" "${INSTALL_DIR}/simadmin" \
    && directories_equal "$STAGED_WWW" "${INSTALL_DIR}/www" \
    && files_equal "$STAGED_META" "${INSTALL_DIR}/meta.json"; then
    APP_CHANGED=0
  fi
  MAIN_UNIT_CHANGED=1
  if files_equal "$STAGED_MAIN_UNIT" "${SYSTEMD_UNIT_DIR}/${SERVICE_NAME}.service"; then
    MAIN_UNIT_CHANGED=0
  fi
  RECOVERY_SCRIPT_CHANGED=1
  if files_equal "$STAGED_RECOVERY_SCRIPT" "${MODEM_RECOVERY_BIN_DIR}/simadmin-modem-recovery.sh"; then
    RECOVERY_SCRIPT_CHANGED=0
  fi
  RECOVERY_UNIT_CHANGED=1
  if files_equal "$STAGED_RECOVERY_UNIT" "${SYSTEMD_UNIT_DIR}/simadmin-modem-recovery.service"; then
    RECOVERY_UNIT_CHANGED=0
  fi
  return 0
}

cleanup_staged_transaction() {
  [ -n "${STAGED_BIN:-}" ] && rm -f -- "$STAGED_BIN"
  [ -n "${STAGED_WWW:-}" ] && rm -rf -- "$STAGED_WWW"
  [ -n "${STAGED_META:-}" ] && rm -f -- "$STAGED_META"
  [ -n "${STAGED_MAIN_UNIT:-}" ] && rm -f -- "$STAGED_MAIN_UNIT"
  [ -n "${STAGED_RECOVERY_SCRIPT:-}" ] && rm -f -- "$STAGED_RECOVERY_SCRIPT"
  [ -n "${STAGED_RECOVERY_UNIT:-}" ] && rm -f -- "$STAGED_RECOVERY_UNIT"
  return 0
}

backup_existing_path() {
  destination="$1"
  backup="$2"
  rm -rf -- "$backup"
  if [ -e "$destination" ] || [ -L "$destination" ]; then
    mv "$destination" "$backup"
    return 0
  fi
  return 1
}

rollback_install() {
  echo "==> rolling back SimAdmin installation" >&2
  systemctl stop "${SERVICE_NAME}.service" >/dev/null 2>&1 || true

  if [ "${TOUCHED_BIN:-0}" -eq 1 ]; then rm -f -- "${INSTALL_DIR}/simadmin"; [ "${HAD_BIN:-0}" -eq 1 ] && mv "$BACKUP_BIN" "${INSTALL_DIR}/simadmin"; fi
  if [ "${TOUCHED_WWW:-0}" -eq 1 ]; then rm -rf -- "${INSTALL_DIR}/www"; [ "${HAD_WWW:-0}" -eq 1 ] && mv "$BACKUP_WWW" "${INSTALL_DIR}/www"; fi
  if [ "${TOUCHED_META:-0}" -eq 1 ]; then rm -f -- "${INSTALL_DIR}/meta.json"; [ "${HAD_META:-0}" -eq 1 ] && mv "$BACKUP_META" "${INSTALL_DIR}/meta.json"; fi
  if [ "${TOUCHED_MAIN_UNIT:-0}" -eq 1 ]; then rm -f -- "${SYSTEMD_UNIT_DIR}/${SERVICE_NAME}.service"; [ "${HAD_MAIN_UNIT:-0}" -eq 1 ] && mv "$BACKUP_MAIN_UNIT" "${SYSTEMD_UNIT_DIR}/${SERVICE_NAME}.service"; fi
  if [ "${TOUCHED_RECOVERY_SCRIPT:-0}" -eq 1 ]; then rm -f -- "${MODEM_RECOVERY_BIN_DIR}/simadmin-modem-recovery.sh"; [ "${HAD_RECOVERY_SCRIPT:-0}" -eq 1 ] && mv "$BACKUP_RECOVERY_SCRIPT" "${MODEM_RECOVERY_BIN_DIR}/simadmin-modem-recovery.sh"; fi
  if [ "${TOUCHED_RECOVERY_UNIT:-0}" -eq 1 ]; then rm -f -- "${SYSTEMD_UNIT_DIR}/simadmin-modem-recovery.service"; [ "${HAD_RECOVERY_UNIT:-0}" -eq 1 ] && mv "$BACKUP_RECOVERY_UNIT" "${SYSTEMD_UNIT_DIR}/simadmin-modem-recovery.service"; fi

  systemctl daemon-reload >/dev/null 2>&1 || true
  if [ "${PREVIOUS_MAIN_ENABLED:-0}" -eq 1 ]; then systemctl enable "${SERVICE_NAME}.service" >/dev/null 2>&1 || true; else systemctl disable "${SERVICE_NAME}.service" >/dev/null 2>&1 || true; fi
  if [ "${PREVIOUS_RECOVERY_ENABLED:-0}" -eq 1 ]; then systemctl enable simadmin-modem-recovery.service >/dev/null 2>&1 || true; else systemctl disable simadmin-modem-recovery.service >/dev/null 2>&1 || true; fi
  if [ "${PREVIOUS_MAIN_ACTIVE:-0}" -eq 1 ]; then systemctl start "${SERVICE_NAME}.service" >/dev/null 2>&1 || true; fi
  TRANSACTION_ACTIVE=0
}

commit_install() {
  TRANSACTION_ACTIVE=0
  for committed_backup in "${BACKUP_BIN:-}" "${BACKUP_WWW:-}" "${BACKUP_META:-}" "${BACKUP_MAIN_UNIT:-}" "${BACKUP_RECOVERY_SCRIPT:-}" "${BACKUP_RECOVERY_UNIT:-}"; do
    if [ -n "$committed_backup" ]; then
      rm -rf -- "$committed_backup" || echo "warning: failed to remove transaction backup ${committed_backup}" >&2
    fi
  done
  cleanup_staged_transaction
}

activate_staged_install() {
  BACKUP_BIN="${INSTALL_DIR}/.simadmin.previous.$$"
  BACKUP_WWW="${INSTALL_DIR}/.www.previous.$$"
  BACKUP_META="${INSTALL_DIR}/.meta.json.previous.$$"
  BACKUP_MAIN_UNIT="${SYSTEMD_UNIT_DIR}/.${SERVICE_NAME}.service.previous.$$"
  BACKUP_RECOVERY_SCRIPT="${MODEM_RECOVERY_BIN_DIR}/.simadmin-modem-recovery.sh.previous.$$"
  BACKUP_RECOVERY_UNIT="${SYSTEMD_UNIT_DIR}/.simadmin-modem-recovery.service.previous.$$"
  HAD_BIN=0; HAD_WWW=0; HAD_META=0; HAD_MAIN_UNIT=0; HAD_RECOVERY_SCRIPT=0; HAD_RECOVERY_UNIT=0
  TOUCHED_BIN=0; TOUCHED_WWW=0; TOUCHED_META=0; TOUCHED_MAIN_UNIT=0; TOUCHED_RECOVERY_SCRIPT=0; TOUCHED_RECOVERY_UNIT=0
  PREVIOUS_MAIN_ACTIVE=0; PREVIOUS_MAIN_ENABLED=0; PREVIOUS_RECOVERY_ENABLED=0
  systemctl is-active --quiet "${SERVICE_NAME}.service" && PREVIOUS_MAIN_ACTIVE=1 || true
  systemctl is-enabled --quiet "${SERVICE_NAME}.service" 2>/dev/null && PREVIOUS_MAIN_ENABLED=1 || true
  systemctl is-enabled --quiet simadmin-modem-recovery.service 2>/dev/null && PREVIOUS_RECOVERY_ENABLED=1 || true
  TRANSACTION_ACTIVE=1

  if [ "$APP_CHANGED" -eq 1 ] || [ "$MAIN_UNIT_CHANGED" -eq 1 ]; then
    echo "==> stopping existing SimAdmin service"
    systemctl stop "${SERVICE_NAME}.service" >/dev/null 2>&1 || true
  fi
  if [ "$APP_CHANGED" -eq 1 ]; then
    echo "==> activating application files"
    TOUCHED_BIN=1; backup_existing_path "${INSTALL_DIR}/simadmin" "$BACKUP_BIN" && HAD_BIN=1 || true; mv "$STAGED_BIN" "${INSTALL_DIR}/simadmin"
    TOUCHED_WWW=1; backup_existing_path "${INSTALL_DIR}/www" "$BACKUP_WWW" && HAD_WWW=1 || true; mv "$STAGED_WWW" "${INSTALL_DIR}/www"
    TOUCHED_META=1; backup_existing_path "${INSTALL_DIR}/meta.json" "$BACKUP_META" && HAD_META=1 || true; mv "$STAGED_META" "${INSTALL_DIR}/meta.json"
  else
    echo "==> application files are unchanged; skipping replacement"
  fi
  if [ "$MAIN_UNIT_CHANGED" -eq 1 ]; then
    TOUCHED_MAIN_UNIT=1; backup_existing_path "${SYSTEMD_UNIT_DIR}/${SERVICE_NAME}.service" "$BACKUP_MAIN_UNIT" && HAD_MAIN_UNIT=1 || true; mv "$STAGED_MAIN_UNIT" "${SYSTEMD_UNIT_DIR}/${SERVICE_NAME}.service"
  fi
  if [ "$RECOVERY_SCRIPT_CHANGED" -eq 1 ]; then
    TOUCHED_RECOVERY_SCRIPT=1; backup_existing_path "${MODEM_RECOVERY_BIN_DIR}/simadmin-modem-recovery.sh" "$BACKUP_RECOVERY_SCRIPT" && HAD_RECOVERY_SCRIPT=1 || true; mv "$STAGED_RECOVERY_SCRIPT" "${MODEM_RECOVERY_BIN_DIR}/simadmin-modem-recovery.sh"
  fi
  if [ "$RECOVERY_UNIT_CHANGED" -eq 1 ]; then
    TOUCHED_RECOVERY_UNIT=1; backup_existing_path "${SYSTEMD_UNIT_DIR}/simadmin-modem-recovery.service" "$BACKUP_RECOVERY_UNIT" && HAD_RECOVERY_UNIT=1 || true; mv "$STAGED_RECOVERY_UNIT" "${SYSTEMD_UNIT_DIR}/simadmin-modem-recovery.service"
  fi

  if [ "$MAIN_UNIT_CHANGED" -eq 1 ] || [ "$RECOVERY_UNIT_CHANGED" -eq 1 ]; then
    systemctl daemon-reload
  fi
  systemctl enable "${SERVICE_NAME}.service" >/dev/null
  systemctl enable simadmin-modem-recovery.service >/dev/null
  if [ "$APP_CHANGED" -eq 1 ] || [ "$MAIN_UNIT_CHANGED" -eq 1 ]; then
    systemctl restart "${SERVICE_NAME}.service"
  elif ! systemctl is-active --quiet "${SERVICE_NAME}.service"; then
    systemctl start "${SERVICE_NAME}.service"
  fi
}

wait_for_simadmin_health() {
  if truthy "$SIMADMIN_SKIP_HEALTHCHECK"; then
    echo "warning: post-install HTTP health check is disabled" >&2
    systemctl is-active --quiet "${SERVICE_NAME}.service"
    return $?
  fi
  health_attempt=0
  health_limit="$SIMADMIN_HEALTH_RETRIES"
  [ "$health_limit" -gt 0 ] || health_limit=1
  while [ "$health_attempt" -lt "$health_limit" ]; do
    if systemctl is-active --quiet "${SERVICE_NAME}.service" \
      && curl -fsS --connect-timeout 2 --max-time 3 "$SIMADMIN_HEALTH_URL" >/dev/null 2>&1; then
      echo "==> SimAdmin health check passed"
      return 0
    fi
    health_attempt=$((health_attempt + 1))
    [ "$health_attempt" -lt "$health_limit" ] && sleep 1
  done
  echo "error: SimAdmin did not become healthy at ${SIMADMIN_HEALTH_URL}" >&2
  systemctl status "${SERVICE_NAME}.service" --no-pager >&2 || true
  return 1
}



main() {
  parse_args "$@"
  require_root
  require_cmd mktemp
  validate_configuration
  validate_running_architecture
  acquire_install_lock
  trap 'installer_exit $?' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  tmp_dir="$(create_temp_dir)"

  if truthy "$SIMADMIN_LPAC_ONLY"; then
    if ! detect_lpac_arch >/dev/null 2>&1; then
      echo "warning: unsupported device arch for lpac: $(uname -m), skipping lpac install" >&2
      return 0
    fi
    install_bootstrap_dependencies
    if truthy "$SIMADMIN_INSTALL_SYSTEM_DEPS" && [ "$SIMADMIN_DEPS_MODE" != "skip" ]; then
      ensure_package_list "lpac dependencies" "libpcsclite1"
    fi
    install_lpac
    return 0
  fi

  require_cmd systemctl
  echo "==> installing SimAdmin"
  echo "    version: ${VERSION}"
  echo "    install dir: ${INSTALL_DIR}"
  echo "    service name: ${SERVICE_NAME}"

  if [ -x "${INSTALL_DIR}/simadmin" ]; then FIRST_INSTALL=0; else FIRST_INSTALL=1; fi
  install_bootstrap_dependencies
  require_cmd curl

  if [ -z "$ASSET_URL" ] && [ -z "${TARGET_TAG:-}" ]; then
    TARGET_TAG="$(resolve_target_tag || true)"
  fi
  if [ -n "${TARGET_TAG:-}" ]; then
    PRIMARY_RELEASE_VERSION="${TARGET_TAG#v}"
  fi

  asset_url="$(resolve_asset_url)"
  case "$asset_url" in
    *.tar.gz)
      require_cmd tar
      archive_path="${tmp_dir}/simadmin.tar.gz"
      ;;
    *)
      echo "error: unsupported OTA asset format, expected .tar.gz: $asset_url" >&2
      return 1
      ;;
  esac

  download_release_asset "$archive_path" "$asset_url"
  if [ -n "$ASSET_NAME" ]; then
    archive_asset_name="$ASSET_NAME"
  elif [ -n "${DOWNLOADED_ASSET_URL:-}" ]; then
    archive_asset_name="${DOWNLOADED_ASSET_URL##*/}"
    archive_asset_name="${archive_asset_name%%\?*}"
    archive_asset_name="${archive_asset_name%%#*}"
  elif [ -n "${TARGET_TAG:-}" ]; then
    archive_asset_name="$(resolve_simadmin_asset_name "$TARGET_TAG")"
  else
    archive_asset_name="$(resolve_simadmin_asset_name)"
  fi
  verify_release_asset "$archive_path" "$archive_asset_name"
  extract_release_package "$archive_path"
  expected_arch="$(expected_target_triple)"
  selected_edition="$(target_edition)"
  validate_package_metadata "$expected_arch" "$selected_edition"
  verify_binary_architecture "${package_dir}/simadmin" "$expected_arch"
  check_install_disk_space

  # Managed unit and recovery files remain in the source repository. Latest
  # installs use main; versioned installs use the corresponding release tag.
  resolve_source_urls "$PACKAGE_VERSION"
  prepare_managed_service_files

  # No service or modem state is changed before all remote inputs are ready.
  install_system_dependencies
  prepare_transaction_files "$selected_edition"
  install_lpac
  remove_legacy_networkmanager_modem_unmanaged
  start_runtime_services
  refresh_modem_devices
  activate_staged_install
  if ! wait_for_simadmin_health; then
    rollback_install
    return 1
  fi
  commit_install

  echo "==> done"
  echo "    service: ${SERVICE_NAME}.service"
  echo "    modem recovery: simadmin-modem-recovery.service"
  echo "    install dir: ${INSTALL_DIR}"
  systemctl status "${SERVICE_NAME}.service" --no-pager || true
}

if [ "${SIMADMIN_INSTALL_LIBRARY_ONLY:-0}" != "1" ]; then
  main "$@"
fi
