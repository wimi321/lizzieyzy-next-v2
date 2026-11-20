#!/usr/bin/env bash
# Install the packages carried by tauri-e2e-Linux; never fetch missing dependencies.
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"

runtime_install_started=$(date +%s)
record_install_time() {
  local status=$?
  local elapsed=$(( $(date +%s) - runtime_install_started ))
  printf 'Linux E2E offline runtime installation: %s seconds (exit %s).\n' "$elapsed" "$status"
  if [ -n "${GITHUB_ENV:-}" ]; then
    printf 'TAURI_E2E_APT_SECONDS=%s\n' "$elapsed" >> "$GITHUB_ENV"
  fi
  if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    printf 'Linux E2E offline runtime installation: %s seconds (exit %s).\n' \
      "$elapsed" "$status" >> "$GITHUB_STEP_SUMMARY"
  fi
}
trap record_install_time EXIT

if [ "$#" -ne 1 ] || [ ! -d "$1" ]; then
  echo '::error::Linux runtime .deb directory is missing; expected the tauri-e2e-Linux artifact.' >&2
  exit 1
fi
runtime_directory=$(cd -- "$1" && pwd)
shopt -s nullglob
runtime_packages=("$runtime_directory"/*.deb)
if [ "${#runtime_packages[@]}" -eq 0 ]; then
  echo "::error::No .deb packages in $runtime_directory; Verify must supply the Linux runtime artifact." >&2
  exit 1
fi

# Local .deb files and already installed packages are the only candidates.
# --no-download fails on an incomplete set instead of falling back to a mirror.
if sudo apt-get install --no-install-recommends --no-download -y \
  -o Dir::Etc::sourcelist=/dev/null -o Dir::Etc::sourceparts=- \
  "${runtime_packages[@]}"; then
  :
else
  runtime_install_status=$?
  echo '::error::Offline Linux runtime installation failed: the .deb artifact may be incomplete or the ubuntu-24.04 runner images may differ. Network fallback is disabled.' >&2
  exit "$runtime_install_status"
fi

if ! command -v WebKitWebDriver >/dev/null; then
  echo '::error::webkit2gtk-driver did not provide WebKitWebDriver after offline installation.' >&2
  exit 1
fi
if ! command -v xvfb-run >/dev/null; then
  echo '::error::xvfb did not provide xvfb-run after offline installation.' >&2
  exit 1
fi
