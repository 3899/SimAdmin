#!/usr/bin/env bash

# Isolated installer and uninstaller regression tests. No host services or
# packages are changed; systemctl, dpkg-query, and apt-get are test doubles.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
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

assert_file_contains() {
    local file="$1"
    local text="$2"
    local label="$3"
    grep -Fq -- "$text" "$file" || fail "$label"
}

assert_file_not_contains() {
    local file="$1"
    local text="$2"
    local label="$3"
    if grep -Fq -- "$text" "$file"; then
        fail "$label"
    fi
}

assert_command_fails() {
    local label="$1"
    shift
    set +e
    "$@" >/dev/null 2>&1
    local status=$?
    set -e
    [ "$status" -ne 0 ] || fail "$label"
}

file_url() {
    if command -v cygpath >/dev/null 2>&1; then
        printf 'file://%s\n' "$(cygpath -m "$1")"
    else
        printf 'file://%s\n' "$1"
    fi
}

make_fake_package_tools() {
    local fake_bin="$1"
    mkdir -p "$fake_bin"

    cat > "$fake_bin/dpkg-query" <<'SH'
#!/bin/sh
format=""
package=""
for argument in "$@"; do
    case "$argument" in
        -f=*) format="${argument#-f=}" ;;
        -*) ;;
        *) package="$argument" ;;
    esac
done
if [ "${package:-}" = "${FAKE_MISSING_PACKAGE:-}" ] \
    && [ ! -f "${FAKE_PACKAGE_STATE}/installed.${package}" ]; then
    exit 1
fi
case "$format" in
    *Status*) printf '%s\n' 'install ok installed' ;;
    *) printf '%s\n' '999.0' ;;
esac
SH

    cat > "$fake_bin/apt-get" <<'SH'
#!/bin/sh
printf '%s\n' "$*" >> "$FAKE_APT_LOG"
if [ "${1:-}" = "install" ]; then
    shift
    for package in "$@"; do
        case "$package" in -*) continue ;; esac
        : > "${FAKE_PACKAGE_STATE}/installed.${package}"
    done
fi
SH
    chmod 0755 "$fake_bin/dpkg-query" "$fake_bin/apt-get"
}

make_fake_systemctl() {
    local fake_bin="$1"
    mkdir -p "$fake_bin"
    cat > "$fake_bin/systemctl" <<'SH'
#!/bin/sh
printf '%s\n' "$*" >> "$SYSTEMCTL_LOG"
case "${1:-}" in
    is-active)
        case "$*" in
            *simadmin-test.service*)
                if [ "${FAKE_FAIL_HEALTH:-0}" = "1" ] \
                    && [ -f "${FAKE_SYSTEMCTL_STATE}/main-restarted" ]; then
                    exit 1
                fi
                ;;
        esac
        exit 0
        ;;
    is-enabled)
        exit 0
        ;;
    restart)
        if [ "${2:-}" = "simadmin-test.service" ]; then
            : > "${FAKE_SYSTEMCTL_STATE}/main-restarted"
        fi
        exit 0
        ;;
    *)
        exit 0
        ;;
esac
SH
    chmod 0755 "$fake_bin/systemctl"
}

make_release_package() {
    local fixture_root="$1"
    local target_arch="$2"
    local version="$3"
    local content="$4"
    local package_root="${fixture_root}/package-root"
    rm -rf "$package_root"
    mkdir -p "$package_root/www"
    printf '%s\n' '#!/bin/sh' "# ${content}" 'exit 0' > "$package_root/simadmin"
    chmod 0755 "$package_root/simadmin"
    printf '%s\n' "$content" > "$package_root/www/index.html"
    printf '{"version":"%s","arch":"%s","edition":"standard"}\n' \
        "$version" "$target_arch" > "$package_root/meta.json"
    tar -czf "${fixture_root}/package.tar.gz" -C "$package_root" meta.json simadmin www
}

make_source_files() {
    local fixture_root="$1"
    local content="$2"
    printf '%s\n' '[Unit]' '[Service]' "Description=${content}" \
        'WorkingDirectory=/opt/simadmin' 'ExecStart=/opt/simadmin/simadmin' \
        > "${fixture_root}/simadmin.service"
    printf '%s\n' '#!/bin/sh' "# ${content}" 'exit 0' \
        > "${fixture_root}/simadmin-modem-recovery.sh"
    chmod 0755 "${fixture_root}/simadmin-modem-recovery.sh"
    printf '%s\n' '[Unit]' '[Service]' "Description=${content}" \
        'ExecStart=/usr/local/bin/simadmin-modem-recovery.sh' \
        > "${fixture_root}/simadmin-modem-recovery.service"
}

run_fixture_installer() {
    local fixture_root="$1"
    local fail_health="${2:-0}"
    local shell_bin="${FIXTURE_SHELL:-bash}"
    PATH="${fixture_root}/fake-bin:$PATH" \
    INSTALL_DIR="${fixture_root}/install" \
    SERVICE_NAME=simadmin-test \
    SIMADMIN_SYSTEMD_UNIT_DIR="${fixture_root}/systemd" \
    SIMADMIN_MODEM_RECOVERY_BIN_DIR="${fixture_root}/bin" \
    SIMADMIN_LOCK_DIR="${fixture_root}/install.lock" \
    TMPDIR="${fixture_root}/missing-tmp" \
    ASSET_URL="$(file_url "${fixture_root}/package.tar.gz")" \
    SERVICE_URL="$(file_url "${fixture_root}/simadmin.service")" \
    MODEM_RECOVERY_SCRIPT_URL="$(file_url "${fixture_root}/simadmin-modem-recovery.sh")" \
    MODEM_RECOVERY_SERVICE_URL="$(file_url "${fixture_root}/simadmin-modem-recovery.service")" \
    SYSTEMCTL_LOG="${fixture_root}/systemctl.log" \
    FAKE_SYSTEMCTL_STATE="${fixture_root}/systemctl-state" \
    FAKE_FAIL_HEALTH="$fail_health" \
    SIMADMIN_DEPS_MODE=skip \
    SIMADMIN_INSTALL_SYSTEM_DEPS=0 \
    SIMADMIN_ENABLE_NETWORKMANAGER=0 \
    SIMADMIN_REFRESH_MODEM_DEVICES=0 \
    SIMADMIN_INSTALL_LPAC=0 \
    SIMADMIN_VERIFY_ASSET=0 \
    SIMADMIN_SKIP_ELF_CHECK=1 \
    SIMADMIN_SKIP_HEALTHCHECK=1 \
    SIMADMIN_INSTALL_LIBRARY_ONLY=1 \
    "$shell_bin" -c '
        id() { printf "%s\n" 0; }
        uname() { printf "%s\n" x86_64; }
        . ./install_latest.sh
        main
    '
}

test_architectures_and_cli() {
    local output status
    output="$(SIMADMIN_INSTALL_LIBRARY_ONLY=1 bash -c '
        . ./install_latest.sh
        for value in aarch64 arm64 aarch64-unknown-linux-musl; do normalize_simadmin_arch "$value"; done
        for value in armv7 armv7l armhf armv7-unknown-linux-musleabihf; do normalize_simadmin_arch "$value"; done
        for value in x86_64 amd64 x86_64-unknown-linux-musl; do normalize_simadmin_arch "$value"; done
    ')"
    assert_eq $'aarch64\naarch64\naarch64\narmv7\narmv7\narmv7\narmv7\nx86_64\nx86_64\nx86_64' \
        "$output" "architecture aliases"

    output="$(SIMADMIN_INSTALL_LIBRARY_ONLY=1 bash -c '
        . ./install_latest.sh
        for RUNTIME_ARCH in aarch64 armv7 x86_64; do expected_target_triple; done
    ')"
    assert_eq $'aarch64-unknown-linux-musl\narmv7-unknown-linux-musleabihf\nx86_64-unknown-linux-musl' \
        "$output" "target triples"

    output="$(SIMADMIN_INSTALL_LIBRARY_ONLY=1 bash -c '
        . ./install_latest.sh
        parse_args --asset wfc
        SIMADMIN_TARGET_ARCH=amd64 resolve_simadmin_asset_name
    ')"
    assert_eq "simadmin-wfc-x86_64.tar.gz" "$output" "--asset wfc selection"

    set +e
    output="$(SIMADMIN_INSTALL_LIBRARY_ONLY=1 bash -c '. ./install_latest.sh; parse_args --asset' 2>&1)"
    status=$?
    set -e
    [ "$status" -ne 0 ] || fail "--asset without a value was accepted"
    case "$output" in *"--asset requires a value"*) ;; *) fail "--asset missing-value error is unclear" ;; esac

    assert_command_fails "target architecture mismatch was accepted" \
        env SIMADMIN_INSTALL_LIBRARY_ONLY=1 SIMADMIN_TARGET_ARCH=amd64 bash -c '
            uname() { printf "%s\n" armv7l; }
            . ./install_latest.sh
            validate_running_architecture
        '

    output="$(VERSION=latest SIMADMIN_INSTALL_LIBRARY_ONLY=1 bash -c \
        '. ./install_latest.sh; resolve_source_ref 9.9.9')"
    assert_eq "main" "$output" "latest source ref"
    output="$(VERSION=1.2.3 SIMADMIN_INSTALL_LIBRARY_ONLY=1 bash -c \
        '. ./install_latest.sh; resolve_source_ref 9.9.9')"
    assert_eq "v1.2.3" "$output" "versioned source ref"

    output="$(SIMADMIN_INSTALL_LIBRARY_ONLY=1 bash -c '
        . ./install_latest.sh
        release_asset_digest simadmin-armv7.tar.gz <<"JSON"
{
  "name": "simadmin-armv7.tar.gz",
  "digest": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
}
JSON
    ')"
    assert_eq "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" \
        "$output" "GitHub asset digest parsing"

    output="$(SIMADMIN_INSTALL_LIBRARY_ONLY=1 bash -c '
        . ./install_latest.sh
        attempt=0
        download_with_proxies() {
            attempt=$((attempt + 1))
            [ "$attempt" -eq 2 ]
        }
        repo_version() { printf "%s\n" 7.8.9; }
        download_release_asset /tmp/not-written \
            https://github.com/3899/SimAdmin/releases/latest/download/simadmin-x86_64.tar.gz
        release_api_url
    ' | tail -n 1)"
    assert_eq "https://api.github.com/repos/3899/SimAdmin/releases/tags/v7.8.9" \
        "$output" "fallback release digest source"
}

test_temp_fallback() {
    local fixture_root created preferred
    fixture_root="$(mktemp -d)"
    created="$(SIMADMIN_INSTALL_LIBRARY_ONLY=1 TMPDIR="${fixture_root}/missing" bash -c '
        . ./install_latest.sh
        created="$(create_temp_dir)"
        printf "%s\n" "$created"
    ')"
    [ -d "$created" ] || fail "temporary directory fallback did not create a directory"
    case "$created" in "${fixture_root}/missing"/*) fail "missing TMPDIR was not bypassed" ;; esac
    rm -rf "$created"

    mkdir -p "$fixture_root/space dir"
    preferred="$(SIMADMIN_INSTALL_LIBRARY_ONLY=1 SIMADMIN_TMPDIR="$fixture_root/space dir" bash -c '
        . ./install_latest.sh
        create_temp_dir
    ')"
    case "$preferred" in "$fixture_root/space dir"/*) ;; *) fail "SIMADMIN_TMPDIR with spaces was not used" ;; esac
    rm -rf "$preferred" "$fixture_root"
}

test_proxy_download_order() {
    local fixture_root attempt_log output expected
    fixture_root="$(mktemp -d)"
    trap 'rm -rf "$fixture_root"' RETURN
    attempt_log="${fixture_root}/attempts.log"
    expected=$'https://proxy-one.example/https://github.com/3899/SimAdmin/releases/latest/download/simadmin-x86_64.tar.gz\nhttps://proxy-two.example/https://github.com/3899/SimAdmin/releases/latest/download/simadmin-x86_64.tar.gz\nhttps://proxy-three.example/https://github.com/3899/SimAdmin/releases/latest/download/simadmin-x86_64.tar.gz\nhttps://github.com/3899/SimAdmin/releases/latest/download/simadmin-x86_64.tar.gz'

    output="$(ATTEMPT_LOG="$attempt_log" DOWNLOAD_PATH="${fixture_root}/asset" \
        GH_PROXY=https://proxy-one.example/ \
        GH_PROXY_FALLBACKS='https://proxy-two.example/ https://proxy-three.example/' \
        SIMADMIN_INSTALL_LIBRARY_ONLY=1 bash -c '
            . ./install_latest.sh
            direct_url=https://github.com/3899/SimAdmin/releases/latest/download/simadmin-x86_64.tar.gz
            curl() {
                request_url=""
                output_path=""
                while [ "$#" -gt 0 ]; do
                    case "$1" in
                        https://*) request_url="$1" ;;
                        -o) shift; output_path="$1" ;;
                    esac
                    shift
                done
                printf "%s\n" "$request_url" >> "$ATTEMPT_LOG"
                [ "$request_url" = "$direct_url" ] || return 1
                printf "%s\n" downloaded > "$output_path"
            }
            download_with_proxies "$direct_url" "$DOWNLOAD_PATH" >/dev/null 2>&1
            cat "$ATTEMPT_LOG"
        ')"
    assert_eq "$expected" "$output" "GitHub asset proxy order"
    assert_eq "downloaded" "$(cat "${fixture_root}/asset")" \
        "GitHub direct fallback did not complete download"

    : > "$attempt_log"
    output="$(ATTEMPT_LOG="$attempt_log" \
        GH_PROXY=https://proxy-one.example/ \
        GH_PROXY_FALLBACKS='https://proxy-two.example/ https://proxy-three.example/' \
        SIMADMIN_INSTALL_LIBRARY_ONLY=1 bash -c '
            . ./install_latest.sh
            direct_url=https://api.github.com/repos/3899/SimAdmin/releases/latest
            curl() {
                request_url=""
                while [ "$#" -gt 0 ]; do
                    case "$1" in https://*) request_url="$1" ;; esac
                    shift
                done
                printf "%s\n" "$request_url" >> "$ATTEMPT_LOG"
                [ "$request_url" = "$direct_url" ] || return 1
                printf "%s\n" "{}"
            }
            read_with_proxies "$direct_url" >/dev/null 2>&1
            cat "$ATTEMPT_LOG"
        ')"
    expected=$'https://proxy-one.example/https://api.github.com/repos/3899/SimAdmin/releases/latest\nhttps://proxy-two.example/https://api.github.com/repos/3899/SimAdmin/releases/latest\nhttps://proxy-three.example/https://api.github.com/repos/3899/SimAdmin/releases/latest\nhttps://api.github.com/repos/3899/SimAdmin/releases/latest'
    assert_eq "$expected" "$output" "GitHub API proxy order"

    : > "$attempt_log"
    output="$(ATTEMPT_LOG="$attempt_log" DOWNLOAD_PATH="${fixture_root}/custom" \
        GH_PROXY=https://proxy-one.example/ \
        GH_PROXY_FALLBACKS=https://proxy-two.example/ \
        SIMADMIN_INSTALL_LIBRARY_ONLY=1 bash -c '
            . ./install_latest.sh
            curl() {
                request_url=""
                output_path=""
                while [ "$#" -gt 0 ]; do
                    case "$1" in
                        https://*) request_url="$1" ;;
                        -o) shift; output_path="$1" ;;
                    esac
                    shift
                done
                printf "%s\n" "$request_url" >> "$ATTEMPT_LOG"
                printf "%s\n" downloaded > "$output_path"
            }
            download_with_proxies https://downloads.example/simadmin.tar.gz \
                "$DOWNLOAD_PATH" >/dev/null 2>&1
            cat "$ATTEMPT_LOG"
        ')"
    assert_eq "https://downloads.example/simadmin.tar.gz" "$output" \
        "non-GitHub URL unexpectedly used a proxy"
}

test_incremental_dependencies() {
    local fixture_root fake_bin apt_log state output
    fixture_root="$(mktemp -d)"
    trap 'rm -rf "$fixture_root"' RETURN
    fake_bin="${fixture_root}/fake-bin"
    apt_log="${fixture_root}/apt.log"
    state="${fixture_root}/state"
    mkdir -p "$state"
    make_fake_package_tools "$fake_bin"

    PATH="$fake_bin:$PATH" FAKE_APT_LOG="$apt_log" FAKE_PACKAGE_STATE="$state" \
        SIMADMIN_INSTALL_LIBRARY_ONLY=1 SIMADMIN_MODEM_PROTOCOL=qmi \
        SIMADMIN_INSTALL_LPAC=0 bash -c '
            . ./install_latest.sh
            install_bootstrap_dependencies
            install_system_dependencies
        ' >/dev/null
    [ ! -s "$apt_log" ] || fail "apt was called although all dependencies were installed"

    : > "$apt_log"
    rm -f "$state"/installed.*
    PATH="$fake_bin:$PATH" FAKE_APT_LOG="$apt_log" FAKE_PACKAGE_STATE="$state" \
        FAKE_MISSING_PACKAGE=libqmi-utils SIMADMIN_INSTALL_LIBRARY_ONLY=1 \
        SIMADMIN_MODEM_PROTOCOL=qmi SIMADMIN_INSTALL_LPAC=0 bash -c '
            . ./install_latest.sh
            install_bootstrap_dependencies
            install_system_dependencies
        ' >/dev/null
    assert_eq "1" "$(grep -c '^update$' "$apt_log")" "apt update count"
    assert_file_contains "$apt_log" "install -y --no-install-recommends libqmi-utils" \
        "incremental install did not contain libqmi-utils"
    assert_file_not_contains "$apt_log" "network-manager libqmi-utils" \
        "incremental install included already-installed packages"

    : > "$apt_log"
    rm -f "$state"/installed.*
    PATH="$fake_bin:$PATH" FAKE_APT_LOG="$apt_log" FAKE_PACKAGE_STATE="$state" \
        FAKE_MISSING_PACKAGE=libmbim-utils SIMADMIN_INSTALL_LIBRARY_ONLY=1 \
        SIMADMIN_APT_UPDATE=never SIMADMIN_MODEM_PROTOCOL=mbim SIMADMIN_INSTALL_LPAC=0 \
        bash -c '. ./install_latest.sh; install_system_dependencies' >/dev/null
    assert_file_not_contains "$apt_log" "update" "SIMADMIN_APT_UPDATE=never ran apt update"
    assert_file_contains "$apt_log" "install -y --no-install-recommends libmbim-utils" \
        "MBIM missing package was not installed"

    output="$(SIMADMIN_INSTALL_LIBRARY_ONLY=1 SIMADMIN_MODEM_PROTOCOL=qmi \
        SIMADMIN_INSTALL_LPAC=0 bash -c '. ./install_latest.sh; required_package_list')"
    case " $output " in *" libqmi-utils "*) ;; *) fail "QMI mode omitted libqmi-utils" ;; esac
    case " $output " in *" libmbim-utils "*) fail "QMI mode included libmbim-utils" ;; esac

    output="$(SIMADMIN_INSTALL_LIBRARY_ONLY=1 SIMADMIN_MODEM_PROTOCOL=mbim \
        SIMADMIN_INSTALL_LPAC=0 bash -c '. ./install_latest.sh; required_package_list')"
    case " $output " in *" libmbim-utils "*) ;; *) fail "MBIM mode omitted libmbim-utils" ;; esac
    case " $output " in *" libqmi-utils "*) fail "MBIM mode included libqmi-utils" ;; esac

    output="$(SIMADMIN_INSTALL_LIBRARY_ONLY=1 SIMADMIN_MODEM_PROTOCOL=qmi \
        SIMADMIN_INSTALL_LPAC=1 bash -c '
            uname() { printf "%s\n" armv7l; }
            . ./install_latest.sh
            required_package_list
        ')"
    case " $output " in *" libpcsclite1 "*) fail "ARMv7 dependencies included lpac/PCSC" ;; esac
}

test_install_idempotency_and_rollback() {
    local fixture_root rollback_root status residue
    fixture_root="$(mktemp -d)"
    rollback_root="$(mktemp -d)"
    trap 'rm -rf "$fixture_root" "$rollback_root"' RETURN

    mkdir -p "$fixture_root/fake-bin" "$fixture_root/systemctl-state" "$fixture_root/install"
    : > "$fixture_root/systemctl.log"
    make_fake_systemctl "$fixture_root/fake-bin"
    make_release_package "$fixture_root" x86_64-unknown-linux-musl 1.2.3 current
    make_source_files "$fixture_root" current
    run_fixture_installer "$fixture_root" >/dev/null
    assert_file_contains "$fixture_root/systemd/simadmin-test.service" \
        "WorkingDirectory=${fixture_root}/install" "custom install dir was not written to unit"
    assert_file_contains "$fixture_root/systemd/simadmin-modem-recovery.service" \
        "ExecStart=${fixture_root}/bin/simadmin-modem-recovery.sh" \
        "custom recovery dir was not written to unit"

    : > "$fixture_root/systemctl.log"
    rm -f "$fixture_root/systemctl-state/main-restarted"
    FIXTURE_SHELL=dash run_fixture_installer "$fixture_root" >/dev/null
    assert_file_not_contains "$fixture_root/systemctl.log" "restart simadmin-test.service" \
        "identical reinstall restarted SimAdmin"
    assert_file_not_contains "$fixture_root/systemctl.log" "stop simadmin-test.service" \
        "identical reinstall stopped SimAdmin"

    mkdir -p "$rollback_root/fake-bin" "$rollback_root/systemctl-state" \
        "$rollback_root/install/www" "$rollback_root/systemd" "$rollback_root/bin"
    : > "$rollback_root/systemctl.log"
    make_fake_systemctl "$rollback_root/fake-bin"
    make_release_package "$rollback_root" x86_64-unknown-linux-musl 2.0.0 new
    make_source_files "$rollback_root" new
    printf '%s\n' old-binary > "$rollback_root/install/simadmin"
    chmod 0755 "$rollback_root/install/simadmin"
    printf '%s\n' old-www > "$rollback_root/install/www/index.html"
    printf '%s\n' old-meta > "$rollback_root/install/meta.json"
    printf '%s\n' old-main-unit > "$rollback_root/systemd/simadmin-test.service"
    printf '%s\n' old-recovery-unit > "$rollback_root/systemd/simadmin-modem-recovery.service"
    printf '%s\n' old-recovery-script > "$rollback_root/bin/simadmin-modem-recovery.sh"
    chmod 0755 "$rollback_root/bin/simadmin-modem-recovery.sh"

    set +e
    run_fixture_installer "$rollback_root" 1 >/dev/null 2>&1
    status=$?
    set -e
    [ "$status" -ne 0 ] || fail "failed health check did not fail installation"
    assert_eq "old-binary" "$(cat "$rollback_root/install/simadmin")" "binary rollback"
    assert_eq "old-www" "$(cat "$rollback_root/install/www/index.html")" "www rollback"
    assert_eq "old-meta" "$(cat "$rollback_root/install/meta.json")" "metadata rollback"
    assert_eq "old-main-unit" "$(cat "$rollback_root/systemd/simadmin-test.service")" "unit rollback"
    assert_eq "old-recovery-unit" \
        "$(cat "$rollback_root/systemd/simadmin-modem-recovery.service")" \
        "recovery unit rollback"
    assert_eq "old-recovery-script" \
        "$(cat "$rollback_root/bin/simadmin-modem-recovery.sh")" \
        "recovery script rollback"
    residue="$(find "$rollback_root/install" "$rollback_root/systemd" "$rollback_root/bin" \
        \( -name '*.new.*' -o -name '*.previous.*' \) -print)"
    [ -z "$residue" ] || fail "transaction residue remained after rollback: $residue"
}

test_archive_validation() {
    local fixture_root status
    fixture_root="$(mktemp -d)"
    trap 'rm -rf "$fixture_root"' RETURN
    mkdir -p "$fixture_root/root/www"
    printf '%s\n' binary > "$fixture_root/root/simadmin"
    printf '%s\n' '{}' > "$fixture_root/root/meta.json"
    printf '%s\n' index > "$fixture_root/root/www/index.html"
    printf '%s\n' extra > "$fixture_root/root/extra"
    tar -czf "$fixture_root/extra.tar.gz" -C "$fixture_root/root" simadmin meta.json www extra
    assert_command_fails "archive with an extra top-level file was accepted" \
        env SIMADMIN_INSTALL_LIBRARY_ONLY=1 TEST_ARCHIVE="$fixture_root/extra.tar.gz" \
        bash -c '. ./install_latest.sh; validate_release_archive "$TEST_ARCHIVE"'

    mkdir -p "$fixture_root/symlink-tar-bin"
    cat > "$fixture_root/symlink-tar-bin/tar" <<'SH'
#!/bin/sh
case "${1:-}" in
    -tzf)
        printf '%s\n' simadmin meta.json www/ www/index.html www/link
        ;;
    -tvzf)
        printf '%s\n' \
            '-rwxr-xr-x root/root 1 2026-01-01 00:00 simadmin' \
            '-rw-r--r-- root/root 1 2026-01-01 00:00 meta.json' \
            'drwxr-xr-x root/root 0 2026-01-01 00:00 www/' \
            '-rw-r--r-- root/root 1 2026-01-01 00:00 www/index.html' \
            'lrwxrwxrwx root/root 0 2026-01-01 00:00 www/link -> /etc/passwd'
        ;;
    *)
        exec "$REAL_TAR" "$@"
        ;;
esac
SH
    chmod 0755 "$fixture_root/symlink-tar-bin/tar"
    assert_command_fails "archive with a symlink was accepted" \
        env PATH="$fixture_root/symlink-tar-bin:$PATH" REAL_TAR="$(command -v tar)" \
        SIMADMIN_INSTALL_LIBRARY_ONLY=1 TEST_ARCHIVE="$fixture_root/link.tar.gz" \
        bash -c '. ./install_latest.sh; validate_release_archive "$TEST_ARCHIVE"'

    mkdir -p "$fixture_root/traversal"
    printf '%s\n' escape > "$fixture_root/traversal/escape"
    tar -czf "$fixture_root/traversal.tar.gz" --transform='s#^escape#../escape#' \
        -C "$fixture_root/traversal" escape
    assert_command_fails "archive with a traversal path was accepted" \
        env SIMADMIN_INSTALL_LIBRARY_ONLY=1 TEST_ARCHIVE="$fixture_root/traversal.tar.gz" \
        bash -c '. ./install_latest.sh; validate_release_archive "$TEST_ARCHIVE"'
}

run_fixture_uninstaller() {
    local fixture_root="$1"
    local shell_bin="${FIXTURE_SHELL:-bash}"
    shift
    PATH="${fixture_root}/fake-bin:$PATH" \
    INSTALL_DIR="${fixture_root}/install" \
    SERVICE_NAME=simadmin-test \
    SIMADMIN_SYSTEMD_UNIT_DIR="${fixture_root}/systemd" \
    SIMADMIN_MODEM_RECOVERY_BIN_DIR="${fixture_root}/bin" \
    SIMADMIN_LOCK_DIR="${fixture_root}/install.lock" \
    NM_CONF="${fixture_root}/nm/99-simadmin-unmanaged-modem.conf" \
    MM_DEBUG_CONF="${fixture_root}/systemd/ModemManager.service.d/99-simadmin-debug.conf" \
    OTA_STAGING_DIR="${fixture_root}/tmp/ota_staging" \
    DEVICE_CONFIG_PATH="${fixture_root}/data/config.json" \
    HUB_AGENT_DB_PATH="${fixture_root}/data/hub-agent.db" \
    SYSTEMCTL_LOG="${fixture_root}/systemctl.log" \
    FAKE_SYSTEMCTL_STATE="${fixture_root}/systemctl-state" \
    SIMADMIN_UNINSTALL_LIBRARY_ONLY=1 \
    "$shell_bin" -c '
        id() { printf "%s\n" 0; }
        . ./uninstall.sh
        main "$@"
    ' uninstall-test "$@"
}

test_uninstaller() {
    local fixture_root
    fixture_root="$(mktemp -d)"
    trap 'rm -rf "$fixture_root"' RETURN
    mkdir -p "$fixture_root/fake-bin" "$fixture_root/systemctl-state" \
        "$fixture_root/install/www" "$fixture_root/install/lpac" "$fixture_root/install/backups" \
        "$fixture_root/systemd/multi-user.target.wants" \
        "$fixture_root/systemd/ModemManager.service.d" "$fixture_root/bin" \
        "$fixture_root/nm" "$fixture_root/tmp/ota_staging" "$fixture_root/data"
    : > "$fixture_root/systemctl.log"
    make_fake_systemctl "$fixture_root/fake-bin"

    printf '%s\n' app > "$fixture_root/install/simadmin"
    printf '%s\n' web > "$fixture_root/install/www/index.html"
    printf '%s\n' lpac > "$fixture_root/install/lpac/lpac"
    printf '%s\n' meta > "$fixture_root/install/meta.json"
    printf '%s\n' database > "$fixture_root/install/data.db"
    printf '%s\n' wal > "$fixture_root/install/data.db-wal"
    printf '%s\n' fallback-config > "$fixture_root/install/config.json"
    printf '%s\n' backup > "$fixture_root/install/backups/backup.tar.gz"
    printf '%s\n' device-config > "$fixture_root/data/config.json"
    printf '%s\n' hub-agent > "$fixture_root/data/hub-agent.db"
    printf '%s\n' hub-wal > "$fixture_root/data/hub-agent.db-wal"
    printf '%s\n' hub-journal > "$fixture_root/data/hub-agent.db-journal"
    printf '%s\n' unit > "$fixture_root/systemd/simadmin-test.service"
    printf '%s\n' unit > "$fixture_root/systemd/simadmin-modem-recovery.service"
    printf '%s\n' link > "$fixture_root/systemd/multi-user.target.wants/simadmin-test.service"
    printf '%s\n' recovery > "$fixture_root/bin/simadmin-modem-recovery.sh"
    printf '%s\n' nm > "$fixture_root/nm/99-simadmin-unmanaged-modem.conf"
    printf '%s\n' mm > "$fixture_root/systemd/ModemManager.service.d/99-simadmin-debug.conf"
    printf '%s\n' staged > "$fixture_root/tmp/ota_staging/file"
    printf '%s\n' residue > "$fixture_root/install/.simadmin.new.123"

    run_fixture_uninstaller "$fixture_root" --keep-user-data >/dev/null
    [ ! -e "$fixture_root/install/simadmin" ] || fail "keep-data left application binary"
    [ ! -e "$fixture_root/install/www" ] || fail "keep-data left frontend"
    [ ! -e "$fixture_root/install/lpac" ] || fail "keep-data left lpac"
    [ ! -e "$fixture_root/install/meta.json" ] || fail "keep-data left metadata"
    [ ! -e "$fixture_root/install/.simadmin.new.123" ] || fail "transaction residue was not removed"
    [ -f "$fixture_root/install/data.db" ] || fail "keep-data removed database"
    [ -f "$fixture_root/install/data.db-wal" ] || fail "keep-data removed SQLite sidecar"
    [ -f "$fixture_root/install/config.json" ] || fail "keep-data removed fallback config"
    [ -f "$fixture_root/install/backups/backup.tar.gz" ] || fail "keep-data removed backups"
    [ -f "$fixture_root/data/config.json" ] || fail "keep-data removed device config"
    [ -f "$fixture_root/data/hub-agent.db" ] || fail "keep-data removed Hub Agent database"
    [ ! -e "$fixture_root/install.lock" ] || fail "uninstaller lock was not released"
    assert_file_contains "$fixture_root/systemctl.log" "restart NetworkManager.service" \
        "NetworkManager was not restarted after its config was removed"
    assert_file_contains "$fixture_root/systemctl.log" "restart ModemManager.service" \
        "ModemManager was not restarted after its override was removed"

    : > "$fixture_root/systemctl.log"
    FIXTURE_SHELL=dash run_fixture_uninstaller "$fixture_root" --keep-user-data >/dev/null
    assert_file_not_contains "$fixture_root/systemctl.log" "restart NetworkManager.service" \
        "idempotent uninstall restarted NetworkManager"
    assert_file_not_contains "$fixture_root/systemctl.log" "restart ModemManager.service" \
        "idempotent uninstall restarted ModemManager"
    assert_file_not_contains "$fixture_root/systemctl.log" "daemon-reload" \
        "idempotent uninstall unnecessarily reloaded systemd"

    run_fixture_uninstaller "$fixture_root" --purge >/dev/null
    [ ! -e "$fixture_root/install" ] || fail "purge left install directory"
    [ ! -e "$fixture_root/data/config.json" ] || fail "purge left device config"
    [ ! -e "$fixture_root/data/hub-agent.db" ] || fail "purge left Hub Agent database"
    [ ! -e "$fixture_root/data/hub-agent.db-wal" ] || fail "purge left Hub Agent SQLite sidecar"
    [ ! -e "$fixture_root/data/hub-agent.db-journal" ] || fail "purge left Hub Agent rollback journal"

    assert_command_fails "uninstaller accepted INSTALL_DIR=/" \
        env SIMADMIN_UNINSTALL_LIBRARY_ONLY=1 INSTALL_DIR=/ bash -c \
        '. ./uninstall.sh; validate_configuration'
    assert_command_fails "uninstaller accepted an arbitrary recovery file" \
        env SIMADMIN_UNINSTALL_LIBRARY_ONLY=1 MODEM_RECOVERY_SCRIPT=/etc/passwd bash -c \
        '. ./uninstall.sh; validate_configuration'

    mkdir -p "$fixture_root/active.lock"
    printf '%s\n' "$$" > "$fixture_root/active.lock/pid"
    assert_command_fails "uninstaller ignored an active shared lock" \
        env SIMADMIN_UNINSTALL_LIBRARY_ONLY=1 SIMADMIN_LOCK_DIR="$fixture_root/active.lock" \
        bash -c '. ./uninstall.sh; validate_configuration; acquire_install_lock'
    [ -f "$fixture_root/active.lock/pid" ] || fail "active shared lock was removed"

    mkdir -p "$fixture_root/stale.lock"
    printf '%s\n' invalid-pid > "$fixture_root/stale.lock/pid"
    SIMADMIN_UNINSTALL_LIBRARY_ONLY=1 SIMADMIN_LOCK_DIR="$fixture_root/stale.lock" \
        bash -c '. ./uninstall.sh; validate_configuration; acquire_install_lock; cleanup_uninstaller'
    [ ! -e "$fixture_root/stale.lock" ] || fail "stale shared lock was not recovered"
}

for script in install_latest.sh uninstall.sh scripts/test-armv7.sh scripts/test-installers.sh; do
    bash -n "$script"
done

test_architectures_and_cli
test_temp_fallback
test_proxy_download_order
test_incremental_dependencies
test_install_idempotency_and_rollback
test_archive_validation
test_uninstaller

echo "PASS: installer and uninstaller regression checks"
