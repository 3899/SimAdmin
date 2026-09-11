#!/usr/bin/env bash

# Fast, host-independent checks for the ARMv7 MVP boundary.
# The script intentionally does not require Rust, Docker, QEMU, or a modem.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT_DIR"

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

assert_eq() {
    local expected="$1"
    local actual="$2"
    local label="$3"
    [ "$expected" = "$actual" ] || fail "$label (expected '$expected', got '$actual')"
}

assert_status() {
    local expected_status="$1"
    shift
    local label="$1"
    shift
    set +e
    "$@"
    local actual_status=$?
    set -e
    [ "$actual_status" -eq "$expected_status" ] || fail "$label (expected status $expected_status, got $actual_status)"
}

file_url() {
    if command -v cygpath >/dev/null 2>&1; then
        printf 'file://%s\n' "$(cygpath -m "$1")"
    else
        printf 'file://%s\n' "$1"
    fi
}

run_install_smoke() {
    local smoke_dir
    smoke_dir="$(mktemp -d)"
    trap 'rm -rf "$smoke_dir"' RETURN

    mkdir -p "$smoke_dir/pkg/www" "$smoke_dir/fake-bin" "$smoke_dir/install"
    printf '%s\n' '#!/bin/sh' 'exit 0' > "$smoke_dir/pkg/simadmin"
    chmod 0755 "$smoke_dir/pkg/simadmin"
    printf '%s\n' '<!doctype html><title>smoke</title>' > "$smoke_dir/pkg/www/index.html"
    printf '%s\n' '{"version":"1.2.3","arch":"armv7-unknown-linux-musleabihf"}' > "$smoke_dir/pkg/meta.json"
    tar -czf "$smoke_dir/package.tar.gz" -C "$smoke_dir/pkg" meta.json simadmin www

    printf '%s\n' '[Unit]' '[Service]' 'WorkingDirectory=/opt/simadmin' 'ExecStart=/opt/simadmin/simadmin' > "$smoke_dir/service"
    printf '%s\n' '#!/bin/sh' 'exit 0' > "$smoke_dir/recovery.sh"
    chmod 0755 "$smoke_dir/recovery.sh"
    printf '%s\n' '[Unit]' '[Service]' 'ExecStart=/usr/local/bin/simadmin-modem-recovery.sh' > "$smoke_dir/recovery.service"
    printf '%s\n' '#!/bin/sh' 'exit 0' > "$smoke_dir/fake-bin/systemctl"
    chmod 0755 "$smoke_dir/fake-bin/systemctl"

    run_installer() {
        local archive="$1"
        PATH="$smoke_dir/fake-bin:$PATH" \
        INSTALL_DIR="$smoke_dir/install" \
        SERVICE_NAME=simadmin-smoke \
        SIMADMIN_SYSTEMD_UNIT_DIR="$smoke_dir/systemd" \
        SIMADMIN_MODEM_RECOVERY_BIN_DIR="$smoke_dir/bin" \
        SIMADMIN_LOCK_DIR="$smoke_dir/install.lock" \
        TMPDIR="$smoke_dir/missing-tmp" \
        ASSET_URL="$(file_url "$archive")" \
        SERVICE_URL="$(file_url "$smoke_dir/service")" \
        MODEM_RECOVERY_SCRIPT_URL="$(file_url "$smoke_dir/recovery.sh")" \
        MODEM_RECOVERY_SERVICE_URL="$(file_url "$smoke_dir/recovery.service")" \
        SIMADMIN_INSTALL_LIBRARY_ONLY=1 \
        SIMADMIN_INSTALL_SYSTEM_DEPS=0 \
        SIMADMIN_DEPS_MODE=skip \
        SIMADMIN_ENABLE_NETWORKMANAGER=0 \
        SIMADMIN_REFRESH_MODEM_DEVICES=0 \
        SIMADMIN_INSTALL_LPAC=1 \
        SIMADMIN_VERIFY_ASSET=0 \
        SIMADMIN_SKIP_ELF_CHECK=1 \
        SIMADMIN_SKIP_HEALTHCHECK=1 \
        bash -c '
            id() { printf "%s\\n" 0; }
            uname() { printf "%s\\n" armv7l; }
            . ./install_latest.sh
            main
        '
    }

    run_installer "$smoke_dir/package.tar.gz"

    [ -x "$smoke_dir/install/simadmin" ] || fail "ARMv7 installer did not install the main binary"
    [ -f "$smoke_dir/install/meta.json" ] || fail "ARMv7 installer did not install meta.json"
    grep -q '"arch":"armv7-unknown-linux-musleabihf"' "$smoke_dir/install/meta.json" \
        || fail "ARMv7 installer changed or lost the package architecture"
    [ ! -e "$smoke_dir/install/lpac" ] || fail "ARMv7 installer unexpectedly installed lpac"

    cp -R "$smoke_dir/pkg" "$smoke_dir/bad-pkg"
    sed -i 's/armv7-unknown-linux-musleabihf/aarch64-unknown-linux-musl/' "$smoke_dir/bad-pkg/meta.json"
    tar -czf "$smoke_dir/bad-package.tar.gz" -C "$smoke_dir/bad-pkg" meta.json simadmin www
    if run_installer "$smoke_dir/bad-package.tar.gz"; then
        fail "ARMv7 installer accepted an AArch64 package"
    fi

    mkdir "$smoke_dir/no-meta-pkg"
    cp -R "$smoke_dir/pkg/simadmin" "$smoke_dir/pkg/www" "$smoke_dir/no-meta-pkg/"
    tar -czf "$smoke_dir/no-meta-package.tar.gz" -C "$smoke_dir/no-meta-pkg" simadmin www
    if run_installer "$smoke_dir/no-meta-package.tar.gz"; then
        fail "ARMv7 installer accepted a package without meta.json.arch"
    fi
}

for script in scripts/build/build.sh scripts/build/pack-ota.sh scripts/build/deploy.sh install_latest.sh; do
    bash -n "$script"
done

# Loading with SIMADMIN_INSTALL_LIBRARY_ONLY prevents the installer entrypoint
# from requiring root/systemd while keeping its architecture helpers testable.
SIMADMIN_INSTALL_LIBRARY_ONLY=1 . ./install_latest.sh

assert_eq "armv7" "$(normalize_simadmin_arch armv7l)" "uname armv7l alias"
assert_eq "armv7" "$(normalize_simadmin_arch armhf)" "Debian armhf alias"
assert_eq "armv7" "$(normalize_simadmin_arch armv7-unknown-linux-musleabihf)" "Rust target alias"
assert_eq "simadmin-armv7.tar.gz" "$(SIMADMIN_TARGET_ARCH=armv7 resolve_simadmin_asset_name)" "standard ARMv7 asset"

WFC=1
VARIANT=wfc
assert_eq "simadmin-wfc-armv7.tar.gz" "$(SIMADMIN_TARGET_ARCH=armv7 resolve_simadmin_asset_name)" "WFC ARMv7 asset"

# An ARMv7 device must never accept an explicit ARM64 lpac override.
uname() {
    printf '%s\n' "armv7l"
}

LPAC_TARGET_ARCH=aarch64
assert_status 1 "ARMv7 rejects lpac override" detect_lpac_arch
assert_status 1 "ARMv7 has no normalized lpac architecture" normalize_lpac_arch armv7l

run_install_smoke

echo "PASS: ARMv7 architecture and lpac boundary checks"
