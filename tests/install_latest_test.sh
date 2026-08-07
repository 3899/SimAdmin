#!/bin/sh

set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
SIMADMIN_INSTALL_LIBRARY_ONLY=1
REPO=monlor/SimAdmin
unset LPAC_COMPAT_RELEASE_BASE_URL
export SIMADMIN_INSTALL_LIBRARY_ONLY
. "${repo_root}/install_latest.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_eq() {
  expected="$1"
  actual="$2"
  message="$3"
  if [ "$actual" != "$expected" ]; then
    fail "${message}: expected '${expected}', got '${actual}'"
  fi
}

LPAC_ASSET_FLAVOR=compat
assert_eq \
  "https://github.com/monlor/SimAdmin/releases/download/lpac" \
  "$LPAC_COMPAT_RELEASE_BASE_URL" \
  "compatibility release must follow REPO"

assert_eq \
  "lpac-linux-x86_64-glibc2.31.zip" \
  "$(resolve_lpac_compat_asset_name x86_64 2.39)" \
  "x86_64 compat asset must use the bundled-libqmi build"

assert_eq \
  "lpac-linux-x86_64-glibc2.31.zip" \
  "$(resolve_lpac_compat_asset_name x86_64 2.31)" \
  "x86_64 compat asset must support the glibc baseline"

LPAC_TARGET_ARCH=x86_64
LPAC_ASSET_NAME=lpac-linux-x86_64-glibc2.31.zip
assert_eq \
  "${LPAC_COMPAT_RELEASE_BASE_URL}/lpac-linux-x86_64-glibc2.31.zip" \
  "$(resolve_lpac_asset_url)" \
  "x86_64 compatibility asset must be downloaded from the SimAdmin release"
LPAC_TARGET_ARCH=
LPAC_ASSET_NAME=

LPAC_ASSET_FLAVOR=with-qmi
assert_eq \
  "lpac-linux-aarch64-with-qmi.zip" \
  "$(resolve_lpac_asset_name aarch64)" \
  "explicit with-qmi flavor"

test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT INT TERM

cat > "${test_dir}/lpac-qmi" <<'EOF'
#!/bin/sh
printf '%s\n' '{"type":"driver","payload":{"LPAC_APDU":["qmi","stdio"],"LPAC_HTTP":["curl","stdio"]}}'
EOF
chmod 0755 "${test_dir}/lpac-qmi"

cat > "${test_dir}/lpac-no-qmi" <<'EOF'
#!/bin/sh
printf '%s\n' '{"type":"driver","payload":{"LPAC_APDU":["pcsc","at","stdio"],"LPAC_HTTP":["curl","stdio"]}}'
EOF
chmod 0755 "${test_dir}/lpac-no-qmi"

lpac_binary_path_usable "${test_dir}/lpac-qmi" \
  || fail "QMI-capable lpac should pass the installer probe"

if lpac_binary_path_usable "${test_dir}/lpac-no-qmi"; then
  fail "lpac without QMI must fail the installer probe"
fi

read_with_proxies() {
  printf '%s\n' '{"version":"v2.3.0","compat_revision":"2","assets":[{"name":"lpac-linux-x86_64-glibc2.31.zip","arch":"x86_64"}]}'
}

compat_existing="${test_dir}/compat-existing"
mkdir -p "$compat_existing"
cp "${test_dir}/lpac-qmi" "${compat_existing}/lpac"
printf '%s\n' 2.3.0 > "${compat_existing}/VERSION.txt"
printf '%s\n' 1 > "${compat_existing}/COMPAT_REVISION.txt"
if ! lpac_install_needed \
  "${compat_existing}/lpac" \
  "${LPAC_COMPAT_RELEASE_BASE_URL}/lpac-linux-x86_64-glibc2.31.zip"; then
  fail "newer compatibility bundle revision must trigger an lpac upgrade"
fi
assert_eq 2 "$LPAC_TARGET_COMPAT_REVISION" \
  "compatibility manifest revision must be retained for installation"

download_with_proxies() {
  :
}

extract_lpac_archive() {
  mkdir -p "$2"
  cp "${test_dir}/lpac-qmi" "$2/lpac"
}

lpac_install_needed() {
  LPAC_INSTALL_REASON="test install"
  return 0
}

install_root="${test_dir}/install-root"
tmp_dir="${test_dir}/install-tmp"
INSTALL_DIR="$install_root"
LPAC_TARGET_ARCH=x86_64
LPAC_ASSET_NAME=lpac-linux-x86_64-glibc2.31.zip
LPAC_ASSET_FLAVOR=compat
LPAC_TARGET_RELEASE_VERSION=
mkdir -p "${INSTALL_DIR}/lpac"
printf '%s\n' old > "${INSTALL_DIR}/lpac/previous-marker"

install_lpac

[ -x "${INSTALL_DIR}/lpac/lpac" ] \
  || fail "validated lpac stage must be activated at the install destination"
[ ! -e "${INSTALL_DIR}/lpac/previous-marker" ] \
  || fail "previous lpac tree must be replaced after successful activation"
[ ! -e "${INSTALL_DIR}/lpac.previous" ] \
  || fail "previous lpac tree must be cleaned after successful activation"

echo "install_latest tests passed"
