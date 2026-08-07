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
  "lpac-linux-x86_64-with-qmi.zip" \
  "$(resolve_lpac_asset_name x86_64)" \
  "x86_64 compat asset must provide QMI"

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
