#!/bin/bash

set -u

TAG="SimAdmin-ModemRecovery"
MODEM_CACHE_DIR="/var/lib/ModemManager"

log() {
  logger -t "$TAG" "$*"
}

cleanup_modemmanager_cache() {
  if [ -d "$MODEM_CACHE_DIR" ]; then
    find "$MODEM_CACHE_DIR" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} \;
    log "ModemManager runtime cache cleaned: ${MODEM_CACHE_DIR}"
  else
    log "ModemManager runtime cache directory not found: ${MODEM_CACHE_DIR}"
  fi
}

modem_present() {
  mmcli -m 0 >/dev/null 2>&1
}

restart_modem_remoteproc() {
  for r in /sys/class/remoteproc/remoteproc*; do
    [ -e "$r/name" ] || continue
    if grep -qi "mss\|modem" "$r/name" 2>/dev/null; then
      echo stop > "$r/state"
      sleep 3
      echo start > "$r/state"
      log "DSP remoteproc restarted: ${r}"
      return 0
    fi
  done

  log "No modem DSP remoteproc node found"
  return 1
}

log "Boot selftest started; waiting 60 seconds for system and modem initialization..."
sleep 60

NEEDS_RESCUE=0
REASON=""

MMCLI_OUT="$(mmcli -m 0 2>&1 || true)"
if echo "$MMCLI_OUT" | grep -qi "No modems were found"; then
  NEEDS_RESCUE=1
  REASON="modem not found"
else
  STATE="$(printf '%s\n' "$MMCLI_OUT" | awk -F': ' '/state/ { print $2; exit }' | tr -d "'")"
  if [ "$STATE" = "failed" ]; then
    NEEDS_RESCUE=1
    REASON="modem state is failed"
  fi
fi

if [ "$NEEDS_RESCUE" -eq 0 ]; then
  if journalctl -u ModemManager --since "1 minute ago" --no-pager 2>/dev/null | grep -qi "UimUninitialized"; then
    NEEDS_RESCUE=1
    REASON="UimUninitialized detected in ModemManager log"
  fi
fi

if [ "$NEEDS_RESCUE" -eq 0 ]; then
  log "Boot selftest passed; modem state is healthy."
  exit 0
fi

log "Boot selftest failed (${REASON}); starting modem rescue..."

systemctl stop ModemManager >/dev/null 2>&1 || true
killall qmi-proxy >/dev/null 2>&1 || true
cleanup_modemmanager_cache

udevadm trigger >/dev/null 2>&1 || true
sleep 3

systemctl start ModemManager >/dev/null 2>&1 || true
log "First-stage rescue completed; waiting 15 seconds before recheck..."
sleep 15

if modem_present; then
  log "Rescue succeeded; modem is available."
  exit 0
fi

log "First-stage rescue failed; restarting modem DSP remoteproc..."
restart_modem_remoteproc || true

sleep 10
systemctl restart ModemManager >/dev/null 2>&1 || true
log "Second-stage rescue completed; modem recovery exiting."
exit 0
