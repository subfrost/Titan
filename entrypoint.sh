#!/bin/sh
set -e

APP_USER="titan"
APP_GROUP="titan"
HOME_DIR="/home/${APP_USER}"
DATA_DIR="${HOME_DIR}/data"

# Allow overriding user/group via environment for portability (e.g., rootless docker)
PUID="${PUID:-}"
PGID="${PGID:-}"
SKIP_CHOWN="${SKIP_CHOWN:-}"

# Ensure data directory exists
mkdir -p "${DATA_DIR}"

# Determine desired and current ownership (UID:GID)
TARGET_UG="$(id -u "${APP_USER}"):$(id -g "${APP_USER}")"
CURRENT_UG="$(stat -c '%u:%g' "${DATA_DIR}" 2>/dev/null || echo '')"

# Optionally remap container user to requested PUID/PGID
if [ -n "${PUID}" ] || [ -n "${PGID}" ]; then
  if [ "$(id -u)" = "0" ]; then
    if [ -n "${PGID}" ] && [ "${PGID}" != "$(id -g "${APP_USER}")" ]; then
      groupmod -o -g "${PGID}" "${APP_GROUP}" || true
    fi
    if [ -n "${PUID}" ] && [ "${PUID}" != "$(id -u "${APP_USER}")" ]; then
      usermod -o -u "${PUID}" "${APP_USER}" || true
    fi
  else
    echo "[entrypoint] Warning: cannot apply PUID/PGID without root"
  fi
fi

# Fix ownership only if not skipped and mismatched
if [ -z "${SKIP_CHOWN}" ] || [ "${SKIP_CHOWN}" = "0" ]; then
  TARGET_UG="$(id -u "${APP_USER}"):$(id -g "${APP_USER}")"
  CURRENT_UG="$(stat -c '%u:%g' "${DATA_DIR}" 2>/dev/null || echo '')"
  if [ "${CURRENT_UG}" != "${TARGET_UG}" ]; then
    echo "[entrypoint] Fixing ownership of ${DATA_DIR} to ${TARGET_UG}"
    chown -R "${APP_USER}:${APP_GROUP}" "${DATA_DIR}" || true
  fi
else
  echo "[entrypoint] Skipping chown per SKIP_CHOWN=${SKIP_CHOWN}"
fi

# Drop privileges if running as root (prefer gosu, fallback to setpriv)
if [ "$(id -u)" = "0" ]; then
  if command -v gosu >/dev/null 2>&1; then
    exec gosu "${APP_USER}" "$@"
  elif command -v setpriv >/dev/null 2>&1; then
    exec setpriv --reuid "${APP_USER}" --regid "${APP_GROUP}" --init-groups -- "$@"
  else
    echo "[entrypoint] Warning: neither gosu nor setpriv available; running as root"
    exec "$@"
  fi
fi

exec "$@"


