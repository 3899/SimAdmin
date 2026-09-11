#!/bin/sh

set -u

TAG="${SIMADMIN_RECOVERY_TAG:-SimAdmin-ModemRecovery}"
MMCLI_BIN="${MMCLI_BIN:-mmcli}"
QMICLI_BIN="${QMICLI_BIN:-qmicli}"
SYSTEMCTL_BIN="${SYSTEMCTL_BIN:-systemctl}"
TIMEOUT_BIN="${TIMEOUT_BIN:-timeout}"
SLEEP_BIN="${SLEEP_BIN:-sleep}"
LOGGER_BIN="${LOGGER_BIN:-logger}"
QMI_DEVICE="${QMI_DEVICE:-}"
STATE_DIR="${STATE_DIR:-/run/simadmin}"

STARTUP_TIMEOUT_SECONDS="${STARTUP_TIMEOUT_SECONDS:-120}"
CHECK_INTERVAL_SECONDS="${CHECK_INTERVAL_SECONDS:-5}"
STALE_CONFIRMATIONS="${STALE_CONFIRMATIONS:-6}"
INITIAL_PROBE_GRACE_SECONDS="${INITIAL_PROBE_GRACE_SECONDS:-45}"
POST_RESTART_TIMEOUT_SECONDS="${POST_RESTART_TIMEOUT_SECONDS:-90}"
QMI_TIMEOUT_SECONDS="${QMI_TIMEOUT_SECONDS:-15}"

STATUS_FILE="${STATE_DIR}/modem-recovery-status"
IN_PROGRESS_FILE="${STATE_DIR}/modem-recovery-in-progress"

log() {
  message="$*"
  printf '%s\n' "$message"
  "$LOGGER_BIN" -t "$TAG" -- "$message" >/dev/null 2>&1 || true
}

set_status() {
  mkdir -p "$STATE_DIR"
  printf '%s\n' "$1" > "$STATUS_FILE"
}

cleanup() {
  rm -f "$IN_PROGRESS_FILE"
}

mm_snapshot() {
  "$MMCLI_BIN" -m any 2>&1 || true
}

mm_has_sim() {
  printf '%s\n' "$1" | grep -Eq \
    'primary sim path:[[:space:]]*/org/freedesktop/ModemManager1/SIM/'
}

mm_is_stale_sim_missing() {
  snapshot="$1"
  if printf '%s\n' "$snapshot" | grep -Eqi \
    'failed reason:[[:space:]]*.*sim-missing'; then
    return 0
  fi

  if [ "${elapsed:-0}" -ge "${INITIAL_PROBE_GRACE_SECONDS:-45}" ]; then
    if printf '%s\n' "$snapshot" | grep -Eqi \
      'No modems were found|couldn.t find modem'; then
      return 0
    fi
  fi

  return 1
}

resolve_qmi_device() {
  if [ -n "$QMI_DEVICE" ] && [ -e "$QMI_DEVICE" ]; then
    printf '%s\n' "$QMI_DEVICE"
    return 0
  fi

  for candidate in /dev/wwan*qmi0 /dev/cdc-wdm*; do
    if [ -e "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

uim_is_ready() {
  qmi_device="$(resolve_qmi_device)" || return 1
  output="$($TIMEOUT_BIN "$QMI_TIMEOUT_SECONDS" "$QMICLI_BIN" \
    -d "$qmi_device" --device-open-proxy --uim-get-card-status 2>&1 || true)"
  printf '%s\n' "$output" | grep -Eq \
    "Card state:[[:space:]]*'present'" || return 1
  printf '%s\n' "$output" | grep -Eq \
    "Application type:[[:space:]]*'usim" || return 1
  printf '%s\n' "$output" | grep -Eq \
    "Application state:[[:space:]]*'ready'"
}

wait_for_mm_sim() {
  timeout_seconds="$1"
  elapsed=0
  while [ "$elapsed" -lt "$timeout_seconds" ]; do
    snapshot="$(mm_snapshot)"
    if mm_has_sim "$snapshot"; then
      return 0
    fi
    "$SLEEP_BIN" "$CHECK_INTERVAL_SECONDS"
    elapsed=$((elapsed + CHECK_INTERVAL_SECONDS))
  done
  return 1
}

trap cleanup EXIT INT TERM
mkdir -p "$STATE_DIR"
set_status "observing"
log "Cold-start modem observation started"

elapsed=0
stale_count=0
while [ "$elapsed" -lt "$STARTUP_TIMEOUT_SECONDS" ]; do
  snapshot="$(mm_snapshot)"
  if mm_has_sim "$snapshot"; then
    set_status "healthy"
    log "ModemManager SIM object is available; recovery is not needed"
    exit 0
  fi

  if uim_is_ready && mm_is_stale_sim_missing "$snapshot"; then
    stale_count=$((stale_count + 1))
    log "QMI reports USIM ready while ModemManager is stale (${stale_count}/${STALE_CONFIRMATIONS})"
    if [ "$stale_count" -ge "$STALE_CONFIRMATIONS" ]; then
      break
    fi
  else
    stale_count=0
  fi

  "$SLEEP_BIN" "$CHECK_INTERVAL_SECONDS"
  elapsed=$((elapsed + CHECK_INTERVAL_SECONDS))
done

if [ "$stale_count" -lt "$STALE_CONFIRMATIONS" ]; then
  set_status "no-safe-action"
  log "No safe automatic recovery condition was confirmed; leaving modem untouched"
  exit 0
fi

touch "$IN_PROGRESS_FILE"
set_status "restarting-modemmanager"
log "Confirmed QMI USIM ready with persistent ModemManager sim-missing; restarting ModemManager once"
if ! "$SYSTEMCTL_BIN" restart ModemManager.service; then
  set_status "restart-command-failed"
  log "ModemManager restart command failed; no further automatic action will be taken"
  exit 1
fi

if ! wait_for_mm_sim "$POST_RESTART_TIMEOUT_SECONDS"; then
  set_status "recovery-failed"
  log "ModemManager did not recover after one restart; MPSS and the operating system will not be restarted automatically"
  exit 1
fi

set_status "recovered"
log "ModemManager SIM object recovered successfully"
exit 0
