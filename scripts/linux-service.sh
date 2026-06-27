#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="trader-server"
SERVICE_USER="trader"
SERVICE_GROUP="trader"
INSTALL_DIR="/opt/trader"
DATA_DIR="/var/lib/trader"
CONFIG_DIR="/etc/trader"
ENV_FILE="${CONFIG_DIR}/trader-server.env"
CONFIG_FILE="${CONFIG_DIR}/config.toml"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}.service"
SERVER_BIN_PATH="${INSTALL_DIR}/trader-server"
CLI_BIN_PATH="${INSTALL_DIR}/trader"
BACKUP_DIR="${INSTALL_DIR}/backups"

# GitHub release configuration. Set TRADER_GITHUB_REPO to owner/name before using
# check/update against your release repo.
REPO="${TRADER_GITHUB_REPO:-owner/trader}"
GITHUB_API="${TRADER_GITHUB_API:-https://api.github.com/repos/${REPO}}"
ASSET_PLATFORM="linux-x86_64"
TMP_DIR_TO_CLEAN=""

usage() {
  cat <<EOF
Usage: sudo ./linux-service.sh <command> [options]

Service Management:
  install     Install or update the systemd service unit
  start       Start the service
  stop        Stop the service
  restart     Restart the service
  status      Show service status
  logs        Follow service logs
  enable      Enable start on boot
  disable     Disable start on boot
  uninstall   Remove the systemd service unit

Release Management:
  check       Check for updates
  update      Download and install a release archive
  rollback    Restore the latest local backup
  version     Show installed version

Options for update:
  --version <ver>  Install a specific tag, for example v0.1.0
  --force          Reinstall even when the target version already matches
  --no-restart     Replace files without stopping or starting the service

Options for rollback:
  --no-restart     Restore files without stopping or starting the service

Environment:
  TRADER_GITHUB_REPO   GitHub repo in owner/name form
  GITHUB_TOKEN         Optional token for GitHub API/download requests

Installed paths:
  Server binary: ${SERVER_BIN_PATH}
  CLI binary:    ${CLI_BIN_PATH}
  Config file:   ${CONFIG_FILE}
  Env file:      ${ENV_FILE}
  Data dir:      ${DATA_DIR}
EOF
}

require_root() {
  if [[ "${EUID}" -ne 0 ]]; then
    echo "This command must run as root. Use sudo." >&2
    exit 1
  fi
}

require_systemd() {
  if ! command -v systemctl >/dev/null 2>&1; then
    echo "systemctl not found. This script requires a systemd-based Linux distribution." >&2
    exit 1
  fi
}

require_commands() {
  local missing=0
  for command_name in "$@"; do
    if ! command -v "${command_name}" >/dev/null 2>&1; then
      echo "Required command not found: ${command_name}" >&2
      missing=1
    fi
  done
  if [[ "${missing}" -ne 0 ]]; then
    exit 1
  fi
}

cleanup_tmp_dir() {
  if [[ -n "${TMP_DIR_TO_CLEAN:-}" ]]; then
    rm -rf -- "${TMP_DIR_TO_CLEAN}"
    TMP_DIR_TO_CLEAN=""
  fi
}

validate_repo_config() {
  if [[ ! "${REPO}" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
    echo "Invalid GitHub repo: ${REPO}" >&2
    echo "Set TRADER_GITHUB_REPO to owner/name, for example your-org/trader." >&2
    exit 1
  fi
}

validate_release_version() {
  local version="$1"
  if [[ -z "${version}" || ! "${version}" =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "Invalid release version: ${version}" >&2
    exit 1
  fi
}

github_api_get() {
  local url="$1"
  local curl_args=(
    -fsSL
    --retry 3
    --connect-timeout 15
    -H "Accept: application/vnd.github+json"
    -H "X-GitHub-Api-Version: 2022-11-28"
  )
  if [[ -n "${GITHUB_TOKEN:-}" ]]; then
    curl_args+=(-H "Authorization: Bearer ${GITHUB_TOKEN}")
  fi
  curl "${curl_args[@]}" "${url}"
}

download_file() {
  local url="$1"
  local output_path="$2"
  local curl_args=(
    -fL
    --retry 3
    --connect-timeout 15
    --output "${output_path}"
  )
  if [[ -n "${GITHUB_TOKEN:-}" ]]; then
    curl_args+=(-H "Authorization: Bearer ${GITHUB_TOKEN}")
  fi
  curl "${curl_args[@]}" "${url}"
}

release_asset_name() {
  local version="$1"
  echo "trader-${version}-${ASSET_PLATFORM}.tar.gz"
}

release_download_url() {
  local version="$1"
  local asset_name="$2"
  echo "https://github.com/${REPO}/releases/download/${version}/${asset_name}"
}

ensure_service_user() {
  if ! getent group "${SERVICE_GROUP}" >/dev/null 2>&1; then
    groupadd --system "${SERVICE_GROUP}"
  fi

  if id -u "${SERVICE_USER}" >/dev/null 2>&1; then
    return
  fi

  local nologin_shell="/usr/sbin/nologin"
  if [[ ! -x "${nologin_shell}" ]]; then
    nologin_shell="/sbin/nologin"
  fi

  useradd \
    --system \
    --gid "${SERVICE_GROUP}" \
    --home-dir "${DATA_DIR}" \
    --shell "${nologin_shell}" \
    "${SERVICE_USER}"
}

prepare_install_dirs() {
  mkdir -p "${INSTALL_DIR}" "${INSTALL_DIR}/configs" "${BACKUP_DIR}" "${DATA_DIR}" "${CONFIG_DIR}"
  chmod 755 "${INSTALL_DIR}"
  ensure_service_user
  chown root:"${SERVICE_GROUP}" "${CONFIG_DIR}"
  chmod 750 "${CONFIG_DIR}"
  chown "${SERVICE_USER}:${SERVICE_GROUP}" "${DATA_DIR}"
  chmod 750 "${DATA_DIR}"
}

write_default_env_file() {
  if [[ -f "${ENV_FILE}" ]]; then
    return
  fi

  cat >"${ENV_FILE}" <<EOF
TRADER_CONFIG=${CONFIG_FILE}
TRADER_DATABASE_URL=sqlite://${DATA_DIR}/trader.sqlite
TRADER_SERVER_BIND=127.0.0.1:8080
RUST_LOG=info
EOF
  chown root:"${SERVICE_GROUP}" "${ENV_FILE}"
  chmod 640 "${ENV_FILE}"
}

write_default_config_file() {
  if [[ -f "${CONFIG_FILE}" ]]; then
    return
  fi

  if [[ -f "${INSTALL_DIR}/configs/trader-server.example.toml" ]]; then
    cp "${INSTALL_DIR}/configs/trader-server.example.toml" "${CONFIG_FILE}"
  else
    cat >"${CONFIG_FILE}" <<EOF
[runtime]
mode = "backtest"
run_id = "server-default"

[database]
url = "sqlite://${DATA_DIR}/trader.sqlite"

[data]
source = "csv"
path = "${DATA_DIR}/datasets/sample/aapl_1d.csv"

[strategy]
name = "moving_average_cross"
symbols = ["US:NASDAQ:AAPL:EQUITY"]
fast_window = 2
slow_window = 3

[portfolio]
initial_cash = "100000"
base_currency = "USD"
order_qty = "1"
max_abs_qty = "100"

[risk]
max_order_notional = "1000000"
min_cash_after_order = "0"
max_exposure = "1000000"
max_drawdown = "1"
max_leverage = "10"
max_margin_used = "0"
trading_halted = false

[broker]
kind = "simulated"
mode = "paper"

[paper]
account_id = "paper"
slippage_bps = "25"
fee_bps = "10"

[live]
enabled = false
EOF
  fi
  chown root:"${SERVICE_GROUP}" "${CONFIG_FILE}"
  chmod 640 "${CONFIG_FILE}"
}

install_service() {
  require_root
  require_systemd

  if [[ ! -x "${SERVER_BIN_PATH}" ]]; then
    echo "Server binary not found: ${SERVER_BIN_PATH}" >&2
    exit 1
  fi

  prepare_install_dirs
  write_default_env_file
  write_default_config_file

  cat >"${SERVICE_FILE}" <<EOF
[Unit]
Description=Trader API Server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${SERVICE_USER}
Group=${SERVICE_GROUP}
WorkingDirectory=${INSTALL_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${SERVER_BIN_PATH}
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=full
ProtectHome=true
ReadWritePaths=${DATA_DIR}

[Install]
WantedBy=multi-user.target
EOF

  systemctl daemon-reload
  systemctl enable "${SERVICE_NAME}"
  echo "Installed ${SERVICE_NAME}."
  echo "Review these files before starting the service:"
  echo "  ${ENV_FILE}"
  echo "  ${CONFIG_FILE}"
}

uninstall_service() {
  require_root
  require_systemd

  systemctl stop "${SERVICE_NAME}" 2>/dev/null || true
  systemctl disable "${SERVICE_NAME}" 2>/dev/null || true
  rm -f "${SERVICE_FILE}"
  systemctl daemon-reload
  echo "Removed ${SERVICE_NAME} service unit."
  echo "Kept binaries, config, env file, data, and backups in place."
}

get_current_version() {
  local version_file="${INSTALL_DIR}/VERSION"
  if [[ -f "${version_file}" ]]; then
    tr -d '\r\n' <"${version_file}"
  else
    echo "unknown"
  fi
}

get_latest_version() {
  require_commands curl sed head
  validate_repo_config

  local latest_response
  latest_response=$(github_api_get "${GITHUB_API}/releases/latest")
  printf '%s\n' "${latest_response}" |
    sed -nE 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/p' |
    head -n 1
}

check_update() {
  local current_version latest_version
  current_version=$(get_current_version)
  latest_version=$(get_latest_version)

  echo "Current version: ${current_version}"
  echo "Latest version: ${latest_version}"

  if [[ "${current_version}" == "${latest_version}" ]]; then
    echo "Already up to date"
    return 0
  fi

  echo "Update available: ${current_version} -> ${latest_version}"
  return 1
}

show_version() {
  echo "Current version: $(get_current_version)"
  echo "Server binary: ${SERVER_BIN_PATH}"
  echo "CLI binary: ${CLI_BIN_PATH}"
}

service_is_active() {
  [[ -f "${SERVICE_FILE}" ]] &&
    command -v systemctl >/dev/null 2>&1 &&
    systemctl is-active --quiet "${SERVICE_NAME}"
}

install_file_atomic() {
  local source_path="$1"
  local target_path="$2"
  local mode="$3"
  local tmp_path="${target_path}.tmp.$$"

  install -m "${mode}" "${source_path}" "${tmp_path}"
  mv -f "${tmp_path}" "${target_path}"
}

write_version_file_atomic() {
  local version="$1"
  local tmp_path="${INSTALL_DIR}/VERSION.tmp.$$"
  printf '%s\n' "${version}" >"${tmp_path}"
  chmod 0644 "${tmp_path}"
  mv -f "${tmp_path}" "${INSTALL_DIR}/VERSION"
}

download_and_verify_release() {
  local version="$1"
  local tmp_dir="$2"
  local package_dir="$3"
  local asset_name archive_path checksum_path archive_url

  asset_name=$(release_asset_name "${version}")
  archive_path="${tmp_dir}/${asset_name}"
  checksum_path="${tmp_dir}/${asset_name}.sha256"
  archive_url=$(release_download_url "${version}" "${asset_name}")

  download_file "${archive_url}" "${archive_path}"
  download_file "${archive_url}.sha256" "${checksum_path}"

  (
    cd "${tmp_dir}"
    sha256sum -c "${asset_name}.sha256"
  )

  mkdir -p "${package_dir}"
  tar -xzf "${archive_path}" -C "${package_dir}"

  if [[ ! -x "${package_dir}/trader-server" || ! -x "${package_dir}/trader" ]]; then
    echo "Invalid release archive: trader-server or trader binary missing" >&2
    exit 1
  fi
}

backup_current_installation() {
  local current_version backup_path copied
  current_version=$(get_current_version)
  backup_path="${BACKUP_DIR}/$(date -u +%Y%m%d%H%M%S)-${current_version}"
  copied=0

  mkdir -p "${backup_path}"
  for entry in trader-server trader VERSION configs linux-service.sh; do
    if [[ -e "${INSTALL_DIR}/${entry}" ]]; then
      cp -a "${INSTALL_DIR}/${entry}" "${backup_path}/${entry}"
      copied=1
    fi
  done

  if [[ "${copied}" -eq 0 ]]; then
    rmdir "${backup_path}"
    return 0
  fi

  echo "${backup_path}"
}

prune_old_backups() {
  if [[ ! -d "${BACKUP_DIR}" ]]; then
    return
  fi

  local backups=()
  mapfile -t backups < <(find "${BACKUP_DIR}" -mindepth 1 -maxdepth 1 -type d | sort -r)

  local index
  for index in "${!backups[@]}"; do
    if ((index >= 5)); then
      rm -rf -- "${backups[${index}]}"
    fi
  done
}

install_release_payload() {
  local package_dir="$1"
  local version="$2"

  prepare_install_dirs

  install_file_atomic "${package_dir}/trader-server" "${SERVER_BIN_PATH}" 0755
  install_file_atomic "${package_dir}/trader" "${CLI_BIN_PATH}" 0755

  if [[ -f "${package_dir}/linux-service.sh" ]]; then
    install_file_atomic "${package_dir}/linux-service.sh" "${INSTALL_DIR}/linux-service.sh" 0755
  fi

  if [[ -d "${package_dir}/configs" ]]; then
    rm -rf "${INSTALL_DIR}/configs"
    cp -a "${package_dir}/configs" "${INSTALL_DIR}/configs"
  fi

  if [[ -f "${package_dir}/VERSION" ]]; then
    install_file_atomic "${package_dir}/VERSION" "${INSTALL_DIR}/VERSION" 0644
  else
    write_version_file_atomic "${version}"
  fi
}

restore_backup_payload() {
  local backup_path="$1"
  if [[ ! -d "${backup_path}" ]]; then
    echo "Backup not found: ${backup_path}" >&2
    exit 1
  fi

  if [[ -f "${backup_path}/trader-server" ]]; then
    install_file_atomic "${backup_path}/trader-server" "${SERVER_BIN_PATH}" 0755
  fi
  if [[ -f "${backup_path}/trader" ]]; then
    install_file_atomic "${backup_path}/trader" "${CLI_BIN_PATH}" 0755
  fi
  if [[ -f "${backup_path}/linux-service.sh" ]]; then
    install_file_atomic "${backup_path}/linux-service.sh" "${INSTALL_DIR}/linux-service.sh" 0755
  fi
  if [[ -d "${backup_path}/configs" ]]; then
    rm -rf "${INSTALL_DIR}/configs"
    cp -a "${backup_path}/configs" "${INSTALL_DIR}/configs"
  fi
  if [[ -f "${backup_path}/VERSION" ]]; then
    install_file_atomic "${backup_path}/VERSION" "${INSTALL_DIR}/VERSION" 0644
  fi
}

latest_backup_path() {
  if [[ ! -d "${BACKUP_DIR}" ]]; then
    return 0
  fi
  find "${BACKUP_DIR}" -mindepth 1 -maxdepth 1 -type d | sort | tail -n 1
}

update_to_latest() {
  require_root
  require_commands curl tar sha256sum mktemp install mv cp date find sort head rmdir sed
  validate_repo_config

  local target_version="" force=0 restart=1
  while [[ "$#" -gt 0 ]]; do
    case "$1" in
      --version)
        target_version="$2"
        shift 2
        ;;
      --force)
        force=1
        shift
        ;;
      --no-restart)
        restart=0
        shift
        ;;
      *)
        echo "Unknown update option: $1" >&2
        exit 1
        ;;
    esac
  done

  if [[ -z "${target_version}" ]]; then
    target_version=$(get_latest_version)
  fi
  validate_release_version "${target_version}"

  local current_version
  current_version=$(get_current_version)
  if [[ "${force}" -eq 0 && "${current_version}" == "${target_version}" ]]; then
    echo "Already installed: ${target_version}"
    return 0
  fi

  local tmp_dir package_dir backup_path was_active
  tmp_dir=$(mktemp -d)
  TMP_DIR_TO_CLEAN="${tmp_dir}"
  trap cleanup_tmp_dir EXIT
  package_dir="${tmp_dir}/package"

  download_and_verify_release "${target_version}" "${tmp_dir}" "${package_dir}"
  backup_path=$(backup_current_installation || true)
  if [[ -n "${backup_path}" ]]; then
    echo "Backup created: ${backup_path}"
  fi

  was_active=0
  if [[ "${restart}" -eq 1 ]] && service_is_active; then
    was_active=1
    systemctl stop "${SERVICE_NAME}"
  fi

  install_release_payload "${package_dir}" "${target_version}"
  write_default_env_file
  write_default_config_file
  prune_old_backups

  if [[ "${restart}" -eq 1 && -f "${SERVICE_FILE}" ]]; then
    systemctl daemon-reload
    if [[ "${was_active}" -eq 1 ]]; then
      systemctl start "${SERVICE_NAME}"
    fi
  fi

  echo "Updated ${SERVICE_NAME}: ${current_version} -> ${target_version}"
  cleanup_tmp_dir
  trap - EXIT
}

rollback_version() {
  require_root
  require_commands install mv find sort tail cp

  local restart=1
  while [[ "$#" -gt 0 ]]; do
    case "$1" in
      --no-restart)
        restart=0
        shift
        ;;
      *)
        echo "Unknown rollback option: $1" >&2
        exit 1
        ;;
    esac
  done

  local backup_path was_active
  backup_path=$(latest_backup_path)
  if [[ -z "${backup_path}" ]]; then
    echo "No local backup found in ${BACKUP_DIR}" >&2
    exit 1
  fi

  was_active=0
  if [[ "${restart}" -eq 1 ]] && service_is_active; then
    was_active=1
    systemctl stop "${SERVICE_NAME}"
  fi

  restore_backup_payload "${backup_path}"

  if [[ "${restart}" -eq 1 && -f "${SERVICE_FILE}" ]]; then
    systemctl daemon-reload
    if [[ "${was_active}" -eq 1 ]]; then
      systemctl start "${SERVICE_NAME}"
    fi
  fi

  echo "Rolled back ${SERVICE_NAME} using ${backup_path}"
}

service_cmd() {
  require_root
  require_systemd
  systemctl "$1" "${SERVICE_NAME}"
}

command="${1:-}"
shift || true

case "${command}" in
  install)
    install_service
    ;;
  start|stop|restart|status|enable|disable)
    service_cmd "${command}"
    ;;
  logs)
    require_root
    require_systemd
    journalctl -u "${SERVICE_NAME}" -f
    ;;
  uninstall)
    uninstall_service
    ;;
  check)
    check_update
    ;;
  update)
    update_to_latest "$@"
    ;;
  rollback)
    rollback_version "$@"
    ;;
  version)
    show_version
    ;;
  ""|-h|--help|help)
    usage
    ;;
  *)
    echo "Unknown command: ${command}" >&2
    usage
    exit 1
    ;;
esac
