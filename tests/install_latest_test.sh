#!/bin/sh

set -eu

repo_root="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
SIMADMIN_INSTALL_LIBRARY_ONLY=1
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

echo "install_latest tests passed"
