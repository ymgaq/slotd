#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: scripts/install.sh [options]

Install or uninstall slotd for single-user local operation.

Options:
  --repo-root PATH           Repository root to build from.
  --profile NAME             Cargo profile to build with. Default: release
  --install-bin-dir PATH     Install directory for slotd and Slurm-style symlinks.
                             Default: $HOME/.local/bin
  --runtime-root PATH        Runtime root used as SLOTD_ROOT.
                             Default: $HOME/.local/share/slotd
  --config-dir PATH          Config directory for env file.
                             Default: $HOME/.config/slotd
  --systemd-user-dir PATH    systemd user unit directory.
                             Default: $HOME/.config/systemd/user
  --cpu-partitions VALUE     SLOTD_CPU_PARTITIONS value. Default: cpu
  --gpu-partitions VALUE     SLOTD_GPU_PARTITIONS value. Default: gpu
  --features VALUE           SLOTD_FEATURES value.
  --notify-cmd VALUE         SLOTD_NOTIFY_CMD value.
  --cgroup-base PATH         SLOTD_CGROUP_BASE value.
  --uninstall                Remove the installed slotd setup instead of installing.
  --purge-runtime            With --uninstall, also remove the runtime root.
  --skip-build               Reuse an existing cargo build output.
  --skip-systemd             Do not install or start a systemd --user unit.
  --help                     Show this help.
EOF
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/.." && pwd)"
profile="release"
install_bin_dir="${HOME}/.local/bin"
runtime_root="${HOME}/.local/share/slotd"
config_dir="${HOME}/.config/slotd"
systemd_user_dir="${HOME}/.config/systemd/user"
cpu_partitions="cpu"
gpu_partitions="gpu"
features=""
notify_cmd=""
cgroup_base=""
uninstall=0
purge_runtime=0
skip_build=0
skip_systemd=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo-root)
      repo_root="$2"
      shift 2
      ;;
    --profile)
      profile="$2"
      shift 2
      ;;
    --install-bin-dir)
      install_bin_dir="$2"
      shift 2
      ;;
    --runtime-root)
      runtime_root="$2"
      shift 2
      ;;
    --config-dir)
      config_dir="$2"
      shift 2
      ;;
    --systemd-user-dir)
      systemd_user_dir="$2"
      shift 2
      ;;
    --cpu-partitions)
      cpu_partitions="$2"
      shift 2
      ;;
    --gpu-partitions)
      gpu_partitions="$2"
      shift 2
      ;;
    --features)
      features="$2"
      shift 2
      ;;
    --notify-cmd)
      notify_cmd="$2"
      shift 2
      ;;
    --cgroup-base)
      cgroup_base="$2"
      shift 2
      ;;
    --uninstall)
      uninstall=1
      shift
      ;;
    --purge-runtime)
      purge_runtime=1
      shift
      ;;
    --skip-build)
      skip_build=1
      shift
      ;;
    --skip-systemd)
      skip_systemd=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

env_file="${config_dir}/slotd.env"
real_binary="${install_bin_dir}/slotd-real"
wrapper_binary="${install_bin_dir}/slotd"
service_file="${systemd_user_dir}/slotd.service"

load_existing_env_value() {
  local key="$1"
  local file="$2"
  [[ -f "${file}" ]] || return 1
  local line
  line="$(grep -E "^${key}=" "${file}" | tail -n 1 || true)"
  [[ -n "${line}" ]] || return 1
  line="${line#*=}"
  if [[ "${line}" == \"*\" && "${line}" == *\" ]]; then
    line="${line:1:${#line}-2}"
    line="${line//\\\"/\"}"
    line="${line//\\\\/\\}"
  fi
  printf '%s' "${line}"
}

if [[ -f "${env_file}" ]]; then
  [[ -n "${features}" ]] || features="$(load_existing_env_value "SLOTD_FEATURES" "${env_file}" || true)"
  [[ -n "${notify_cmd}" ]] || notify_cmd="$(load_existing_env_value "SLOTD_NOTIFY_CMD" "${env_file}" || true)"
  [[ -n "${cgroup_base}" ]] || cgroup_base="$(load_existing_env_value "SLOTD_CGROUP_BASE" "${env_file}" || true)"
fi

if [[ "${uninstall}" -eq 1 ]]; then
  if [[ "${skip_systemd}" -eq 0 ]] && command -v systemctl >/dev/null 2>&1; then
    systemctl --user disable --now slotd.service >/dev/null 2>&1 || true
    systemctl --user daemon-reload || true
  fi

  rm -f "${service_file}"
  rm -f "${systemd_user_dir}/default.target.wants/slotd.service"
  rm -f "${env_file}"
  rm -f "${wrapper_binary}" "${real_binary}"
  for alias in sbatch srun salloc squeue sacct scontrol scancel sinfo; do
    rm -f "${install_bin_dir}/${alias}"
  done

  if [[ "${purge_runtime}" -eq 1 ]]; then
    rm -rf "${runtime_root}"
  fi

  cat <<EOF
slotd uninstall complete.

Removed binaries and command aliases from:
  ${install_bin_dir}

Removed environment file:
  ${env_file}
EOF

  if [[ "${skip_systemd}" -eq 0 ]]; then
    cat <<EOF

Removed systemd --user unit:
  ${service_file}
EOF
  fi

  if [[ "${purge_runtime}" -eq 1 ]]; then
    cat <<EOF

Removed runtime root:
  ${runtime_root}
EOF
  else
    cat <<EOF

Runtime root was left in place:
  ${runtime_root}

Use --purge-runtime with --uninstall if you also want to remove persisted jobs and state.
EOF
  fi

  exit 0
fi

if [[ ! -f "${repo_root}/Cargo.toml" ]]; then
  echo "error: Cargo.toml not found under repo root: ${repo_root}" >&2
  exit 1
fi

case "${profile}" in
  release|dev)
    ;;
  *)
    echo "error: unsupported profile: ${profile}" >&2
    exit 1
    ;;
esac

binary_src="${repo_root}/target/${profile}/slotd"
if [[ "${skip_build}" -eq 0 ]]; then
  build_args=(cargo build)
  if [[ "${profile}" == "release" ]]; then
    build_args+=(--release)
  fi
  (
    cd "${repo_root}"
    "${build_args[@]}"
  )
fi

if [[ ! -x "${binary_src}" ]]; then
  echo "error: built binary not found: ${binary_src}" >&2
  exit 1
fi

mkdir -p "${install_bin_dir}" "${runtime_root}/run" "${runtime_root}/lib/jobs" "${config_dir}"

escape_env_value() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

write_env() {
  local key="$1"
  local value="$2"
  printf '%s="%s"\n' "${key}" "$(escape_env_value "${value}")" >> "${env_file}"
}

: > "${env_file}"
write_env "SLOTD_ROOT" "${runtime_root}"
write_env "SLOTD_CPU_PARTITIONS" "${cpu_partitions}"
write_env "SLOTD_GPU_PARTITIONS" "${gpu_partitions}"

append_env_if_set() {
  local key="$1"
  local value="$2"
  if [[ -n "${value}" ]]; then
    write_env "${key}" "${value}"
  fi
}

append_env_if_set "SLOTD_FEATURES" "${features}"
append_env_if_set "SLOTD_NOTIFY_CMD" "${notify_cmd}"
append_env_if_set "SLOTD_CGROUP_BASE" "${cgroup_base}"

real_binary_tmp="${real_binary}.tmp.$$"
wrapper_binary_tmp="${wrapper_binary}.tmp.$$"

install -m 0755 "${binary_src}" "${real_binary_tmp}"
cat > "${wrapper_binary_tmp}" <<EOF
#!/usr/bin/env bash
set -euo pipefail
if [[ -f "${env_file}" ]]; then
  set -a
  . "${env_file}"
  set +a
fi
exec -a "\$(basename "\$0")" "${real_binary}" "\$@"
EOF
chmod 0755 "${wrapper_binary_tmp}"
mv -f "${real_binary_tmp}" "${real_binary}"
mv -f "${wrapper_binary_tmp}" "${wrapper_binary}"

for alias in sbatch srun salloc squeue sacct scontrol scancel sinfo; do
  ln -sf "${wrapper_binary}" "${install_bin_dir}/${alias}"
done

if [[ "${skip_systemd}" -eq 0 ]]; then
  mkdir -p "${systemd_user_dir}"
  cat > "${service_file}" <<EOF
[Unit]
Description=slotd daemon
After=default.target

[Service]
ExecStart=${install_bin_dir}/slotd daemon
EnvironmentFile=${env_file}
Restart=always
RestartSec=1

[Install]
WantedBy=default.target
EOF

  if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload
    systemctl --user enable --now slotd.service
  else
    echo "warning: systemctl not found; service file was written but not started" >&2
  fi
fi

cat <<EOF
slotd installation complete.

Installed binary:
  ${wrapper_binary}

Installed payload binary:
  ${real_binary}

Installed command aliases:
  ${install_bin_dir}/sbatch
  ${install_bin_dir}/srun
  ${install_bin_dir}/salloc
  ${install_bin_dir}/squeue
  ${install_bin_dir}/sacct
  ${install_bin_dir}/scontrol
  ${install_bin_dir}/scancel
  ${install_bin_dir}/sinfo

Runtime root:
  ${runtime_root}

Environment file:
  ${env_file}
EOF

if [[ "${skip_systemd}" -eq 0 ]]; then
  cat <<EOF

systemd --user unit:
  ${systemd_user_dir}/slotd.service
EOF
fi

if [[ ":${PATH}:" != *":${install_bin_dir}:"* ]]; then
  cat <<EOF

warning:
  ${install_bin_dir} is not currently on PATH in this shell.
  Add it to your shell startup file if needed.
EOF
fi
