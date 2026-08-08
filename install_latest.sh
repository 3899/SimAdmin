#!/bin/sh

set -eu

REPO="${REPO:-3899/SimAdmin}"
INSTALL_DIR="${INSTALL_DIR:-/opt/simadmin}"
SERVICE_NAME="${SERVICE_NAME:-simadmin}"
VERSION="${VERSION:-latest}"
GH_PROXY="${GH_PROXY:-https://gh-proxy.com/}"
GH_PROXY_FALLBACKS="${GH_PROXY_FALLBACKS:-https://ghproxy.net/ https://githubproxy.cc/}"
RAW_BASE="${RAW_BASE:-https://raw.githubusercontent.com/${REPO}}"
SERVICE_URL="${SERVICE_URL:-${RAW_BASE}/main/scripts/simadmin.service}"
MODEM_RECOVERY_SCRIPT_URL="${MODEM_RECOVERY_SCRIPT_URL:-${RAW_BASE}/main/scripts/simadmin-modem-recovery.sh}"
MODEM_RECOVERY_SERVICE_URL="${MODEM_RECOVERY_SERVICE_URL:-${RAW_BASE}/main/scripts/simadmin-modem-recovery.service}"
ASSET_URL="${ASSET_URL:-}"
WFC="${WFC:-0}"
VARIANT="${VARIANT:-}"

normalize_asset_name() {
  case "$1" in
    wfc|simadmin-wfc|simadmin-wfc.tar.gz)
      printf '%s\n' "simadmin-wfc.tar.gz"
      ;;
    ""|default|standard|simadmin|simadmin.tar.gz)
      printf '%s\n' "simadmin.tar.gz"
      ;;
    *.tar.gz)
      printf '%s\n' "$1"
      ;;
    *)
      printf '%s.tar.gz\n' "$1"
      ;;
  esac
}

if truthy "$WFC" || [ "$VARIANT" = "wfc" ]; then
  ASSET_NAME="simadmin-wfc.tar.gz"
fi
ASSET_NAME="$(normalize_asset_name "${ASSET_NAME:-simadmin.tar.gz}")"

usage() {
  printf '%s\n' \
    'SimAdmin install / upgrade script' \
    '' \
    'Usage:' \
    '  sh install_latest.sh [options] [version]' \
    '' \
    'Examples:' \
    '  sh install_latest.sh                        # Install latest standard release (simadmin.tar.gz)' \
    '  sh install_latest.sh --wfc                  # Install latest Wi-Fi Calling release (simadmin-wfc.tar.gz)' \
    '  sh install_latest.sh -v1.1.8 --wfc          # Install v1.1.8 Wi-Fi Calling release' \
    '  curl -fsSL .../install_latest.sh | WFC=1 sh # Install latest WFC release via env' \
    '' \
    'Options:' \
    '  -v, --version VERSION  Target version to install (default: latest)' \
    '  --wfc                  Install Wi-Fi Calling release asset (simadmin-wfc.tar.gz)' \
    '  -a, --asset NAME       Specify release asset (e.g. simadmin-wfc.tar.gz or wfc)' \
    '  --install-dir PATH     Installation directory (default: /opt/simadmin)' \
    '  --service-name NAME    Main systemd service name (default: simadmin)' \
    '  --no-lpac              Skip lpac installation' \
    '  -h, --help             Show this help' \
    '' \
    'Environment Variables:' \
    '  VERSION=latest         Specify version' \
    '  WFC=1 / VARIANT=wfc    Install Wi-Fi Calling release (simadmin-wfc.tar.gz)' \
    '  ASSET_NAME=...         Specify release asset filename (default: simadmin.tar.gz)' \
    '  INSTALL_DIR=/opt/simadmin' \
    '  SERVICE_NAME=simadmin' \
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
      --wfc)
        ASSET_NAME="simadmin-wfc.tar.gz"
        ;;
      -a|--asset|--variant)
        shift
        if [ "$#" -eq 0 ]; then
          echo "error: $1 requires a value" >&2
          exit 1
        fi
        ASSET_NAME="$(normalize_asset_name "$1")"
        ;;
      -a=*|--asset=*|--variant=*)
        ASSET_NAME="$(normalize_asset_name "${1#*=}")"
        ;;
      -a*)
        ASSET_NAME="$(normalize_asset_name "${1#-a}")"
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
      --no-lpac|--skip-lpac)
        SIMADMIN_INSTALL_LPAC=0
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

truthy() {
  case "$1" in
    1|true|TRUE|yes|YES|y|Y|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

download_with_proxies() {
  src_url="$1"
  dst_path="$2"

  case "$src_url" in
    https://github.com/*|https://raw.githubusercontent.com/*|https://objects.githubusercontent.com/*|https://api.github.com/*)
      for proxy in $GH_PROXY $GH_PROXY_FALLBACKS ""; do
        url="${proxy}${src_url}"
        echo "    ${url}"
        if curl -fsSL "$url" -o "$dst_path"; then
          return 0
        fi
        echo "    download failed, trying next mirror" >&2
      done
      ;;
    *)
      echo "    ${src_url}"
      curl -fsSL "$src_url" -o "$dst_path"
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
        if curl -fsSL "$url"; then
          return 0
        fi
        echo "    download failed, trying next mirror" >&2
      done
      ;;
    *)
      echo "    ${src_url}" >&2
      curl -fsSL "$src_url"
      return $?
      ;;
  esac

  return 1
}

version_to_tag() {
  case "$1" in
    v*) printf '%s\n' "$1" ;;
    *) printf 'v%s\n' "$1" ;;
  esac
}

asset_url_from_tag() {
  tag="$1"
  printf 'https://github.com/%s/releases/download/%s/%s\n' "$REPO" "$tag" "$ASSET_NAME"
}

repo_version() {
  version_text="$(read_with_proxies "${RAW_BASE}/main/VERSION" | tr -d '[:space:]')"
  if [ -z "$version_text" ]; then
    return 1
  fi
  printf '%s\n' "$version_text"
}

resolve_asset_url() {
  if [ -n "$ASSET_URL" ]; then
    printf '%s\n' "$ASSET_URL"
    return 0
  fi

  if [ "$VERSION" = "latest" ]; then
    printf 'https://github.com/%s/releases/latest/download/%s\n' "$REPO" "$ASSET_NAME"
  else
    asset_url_from_tag "$(version_to_tag "$VERSION")"
  fi
}

fallback_asset_url() {
  if [ "$VERSION" = "latest" ] && [ -z "$ASSET_URL" ]; then
    if version_text="$(repo_version)"; then
      asset_url_from_tag "$(version_to_tag "$version_text")"
      return 0
    fi
  fi

  return 1
}

download_release_asset() {
  archive_path="$1"
  primary_url="$2"
  fallback_url=""

  echo "==> downloading release asset"
  if download_with_proxies "$primary_url" "$archive_path"; then
    return 0
  fi

  if fallback_url="$(fallback_asset_url)" && [ "$fallback_url" != "$primary_url" ]; then
    echo "==> latest asset alias download failed, trying versioned asset"
    if download_with_proxies "$fallback_url" "$archive_path"; then
      return 0
    fi
  fi

  echo "error: failed to download OTA asset" >&2
  echo "       tried: $primary_url" >&2
  if [ -n "$fallback_url" ]; then
    echo "       tried: $fallback_url" >&2
  fi
  exit 1
}

install_service_file() {
  service_dst="/etc/systemd/system/${SERVICE_NAME}.service"
  mkdir -p /etc/systemd/system
  download_with_proxies "$SERVICE_URL" "$service_dst"
  systemctl daemon-reload
  systemctl enable "${SERVICE_NAME}.service" >/dev/null
}

install_modem_recovery_service() {
  script_dst="/usr/local/bin/simadmin-modem-recovery.sh"
  service_dst="/etc/systemd/system/simadmin-modem-recovery.service"

  mkdir -p /usr/local/bin /etc/systemd/system
  download_with_proxies "$MODEM_RECOVERY_SCRIPT_URL" "$script_dst"
  chmod 0755 "$script_dst"
  download_with_proxies "$MODEM_RECOVERY_SERVICE_URL" "$service_dst"
  systemctl daemon-reload
  systemctl enable simadmin-modem-recovery.service >/dev/null
}

configure_networkmanager_modem_unmanaged() {
  if [ ! -d /etc/NetworkManager ]; then
    return 0
  fi

  echo "==> configuring NetworkManager to ignore wwan modem"
  mkdir -p /etc/NetworkManager/conf.d
  nm_conf="/etc/NetworkManager/conf.d/99-simadmin-unmanaged-modem.conf"
  {
    printf '%s\n' '[keyfile]'
    printf '%s\n' 'unmanaged-devices=interface-name:wwan*'
  } > "$nm_conf"

  if systemctl is-active --quiet NetworkManager.service; then
    systemctl restart NetworkManager.service || true
  fi
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
      if [ "$arch" = "aarch64" ] && version_le "2.31" "$glibc_version"; then
        printf 'lpac-linux-aarch64-glibc2.31.zip\n'
      else
        printf 'lpac-linux-%s.zip\n' "$arch"
      fi
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

resolve_lpac_asset_url() {
  if [ -n "$LPAC_ASSET_URL" ]; then
    printf '%s\n' "$LPAC_ASSET_URL"
    return 0
  fi

  arch="$(detect_lpac_arch)" || return 1
  asset_name="$(resolve_lpac_asset_name "$arch")" || return 1
  if [ "$LPAC_ASSET_FLAVOR" = "compat" ] && [ "$asset_name" = "lpac-linux-aarch64-glibc2.31.zip" ]; then
    printf '%s/%s\n' "$LPAC_COMPAT_RELEASE_BASE_URL" "$asset_name"
    return 0
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

  # Use simadmin's built-in zip extractor if external tools are unavailable.
  if [ -x "${INSTALL_DIR}/simadmin" ]; then
    echo "    using simadmin extract-zip (built-in)"
    "${INSTALL_DIR}/simadmin" extract-zip "$archive" "$target"
    return $?
  fi

  echo "warning: no zip extractor available, skipping lpac install" >&2
  return 1
}

copy_lpac_tree() {
  extract_dir="$1"
  lpac_dst="$2"
  asset_url="$3"

  if [ -f "${extract_dir}/lpac" ]; then
    bundle_root="${extract_dir}"
  elif [ -f "${extract_dir}/executables/lpac" ]; then
    bundle_root="${extract_dir}/executables"
  else
    bundle_root="$(find "$extract_dir" -type f -name lpac -exec dirname {} \; | head -n 1 || true)"
  fi

  if [ -z "$bundle_root" ] || [ ! -f "${bundle_root}/lpac" ]; then
    echo "warning: downloaded lpac asset does not contain lpac executable" >&2
    return 1
  fi

  rm -rf "${lpac_dst}"
  mkdir -p "${lpac_dst}"
  cp -R "${bundle_root}/." "${lpac_dst}/"

  if [ -d "${extract_dir}/lib" ] && [ ! -d "${lpac_dst}/lib" ]; then
    mkdir -p "${lpac_dst}/lib"
    cp -R "${extract_dir}/lib/." "${lpac_dst}/lib/"
  fi

  if [ -d "${extract_dir}/libraries" ] && [ ! -d "${lpac_dst}/lib" ]; then
    mkdir -p "${lpac_dst}/lib"
    cp -R "${extract_dir}/libraries/." "${lpac_dst}/lib/"
  fi

  chmod -R a+rX "${lpac_dst}"
  chmod 0755 "${lpac_dst}/lpac"

  cat > "${lpac_dst}/SOURCE.txt" <<EOF
lpac is installed from:
${asset_url}

Project:
https://github.com/estkme-group/lpac
EOF
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

  output=$(LD_LIBRARY_PATH="$(lpac_env_prefix "$lpac_path")" "$lpac_path" 2>&1 || true)
  case "$output" in
    *GLIBC_*|*No\ such\ file\ or\ directory*)
      return 1
      ;;
  esac

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
    "$LPAC_COMPAT_RELEASE_BASE_URL"/*|https://github.com/3899/SimAdmin/releases/download/lpac/*)
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

lpac_install_needed() {
  lpac_path="$1"
  lpac_url="$2"
  LPAC_INSTALL_REASON=""
  LPAC_TARGET_RELEASE_VERSION=""
  LPAC_TARGET_RELEASE_SOURCE=""

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

  LPAC_TARGET_RELEASE_VERSION="$(resolve_lpac_target_version "$lpac_url" || true)"
  if [ -z "$LPAC_TARGET_RELEASE_VERSION" ]; then
    LPAC_INSTALL_REASON="latest version could not be verified"
    return 0
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

  if copy_lpac_tree "$lpac_extract" "$lpac_dst" "$lpac_url"; then
    detected_version="$(lpac_command_version "${lpac_dst}/lpac" || true)"
    if [ -z "$detected_version" ]; then
      detected_version="$LPAC_TARGET_RELEASE_VERSION"
    fi
    write_lpac_version_file "$lpac_dst" "$detected_version"
    if lpac_binary_usable "$lpac_dst"; then
      if [ -n "$detected_version" ]; then
        echo "==> lpac ${detected_version} installed to ${lpac_dst}"
      else
        echo "==> lpac installed to ${lpac_dst}"
      fi
    else
      echo "warning: lpac was installed but may not be executable on this device; check glibc/architecture compatibility" >&2
    fi
  else
    echo "warning: failed to install lpac, keeping existing lpac if present" >&2
  fi
}



main() {
  parse_args "$@"
  require_root
  require_cmd curl
  require_cmd systemctl
  require_cmd mktemp

  echo "==> installing SimAdmin"
  echo "    version: ${VERSION}"
  echo "    asset: ${ASSET_NAME}"
  echo "    install dir: ${INSTALL_DIR}"
  echo "    service name: ${SERVICE_NAME}"

  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT INT TERM

  asset_url="$(resolve_asset_url)"

  case "$asset_url" in
    *.tar.gz)
      require_cmd tar
      archive_path="${tmp_dir}/simadmin.tar.gz"
      ;;
    *)
      echo "error: unsupported OTA asset format, expected .tar.gz: $asset_url" >&2
      exit 1
      ;;
  esac

  download_release_asset "$archive_path" "$asset_url"

  echo "==> extracting package"
  mkdir -p "${tmp_dir}/pkg"
  tar -xzf "$archive_path" -C "${tmp_dir}/pkg"

  if [ ! -f "${tmp_dir}/pkg/simadmin" ]; then
    echo "error: invalid package, missing simadmin binary" >&2
    exit 1
  fi

  if [ ! -d "${tmp_dir}/pkg/www" ]; then
    echo "error: invalid package, missing frontend www directory" >&2
    exit 1
  fi

  echo "==> stopping existing service"
  systemctl stop "${SERVICE_NAME}.service" >/dev/null 2>&1 || true

  echo "==> installing files to ${INSTALL_DIR}"
  mkdir -p "${INSTALL_DIR}"
  install -m 0755 "${tmp_dir}/pkg/simadmin" "${INSTALL_DIR}/simadmin"
  rm -rf "${INSTALL_DIR}/www"
  cp -R "${tmp_dir}/pkg/www" "${INSTALL_DIR}/www"
  chmod -R a+rX "${INSTALL_DIR}/www"

  target_edition="standard"
  case "$ASSET_NAME" in
    *wfc*) target_edition="wfc" ;;
  esac

  if [ -f "${tmp_dir}/pkg/meta.json" ]; then
    install -m 0644 "${tmp_dir}/pkg/meta.json" "${INSTALL_DIR}/meta.json"
    if ! grep -q '"edition"' "${INSTALL_DIR}/meta.json"; then
      sed -i "s/}/, \"edition\": \"${target_edition}\"}/" "${INSTALL_DIR}/meta.json" || true
    fi
  else
    cat > "${INSTALL_DIR}/meta.json" << EOF
{
  "version": "${VERSION}",
  "edition": "${target_edition}"
}
EOF
    chmod 0644 "${INSTALL_DIR}/meta.json"
  fi

  install_lpac

  echo "==> installing systemd unit"
  install_service_file
  echo "==> installing modem recovery service"
  install_modem_recovery_service

  configure_networkmanager_modem_unmanaged

  echo "==> starting service"
  systemctl restart "${SERVICE_NAME}.service"

  echo "==> done"
  echo "    service: ${SERVICE_NAME}.service"
  echo "    modem recovery: simadmin-modem-recovery.service"
  echo "    install dir: ${INSTALL_DIR}"
  systemctl status "${SERVICE_NAME}.service" --no-pager
}

main "$@"
