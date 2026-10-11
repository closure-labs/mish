#!/usr/bin/env bash
set -euo pipefail

# Launch nix develop inside a bounded scope. Share one local slot across Mish
# worktrees so concurrent agent shells cannot start overlapping evaluations.
if [ "$#" -eq 0 ]; then
  printf 'Usage: bash scripts/local-nix.sh <nix arguments> | --command <command>\n' >&2
  exit 64
fi
lock_dir=${XDG_RUNTIME_DIR:-}
if [ -z "$lock_dir" ] || [ ! -d "$lock_dir" ] || [ ! -O "$lock_dir" ] || [ -L "$lock_dir" ]; then
  printf 'A private XDG_RUNTIME_DIR is required; use GitHub CI on this host.\n' >&2
  exit 69
fi
if [ "$(stat -c %a "$lock_dir")" != 700 ]; then
  printf 'XDG_RUNTIME_DIR must have owner-only permissions.\n' >&2
  exit 69
fi
exec 9>"$lock_dir/mish-local-nix-$UID.lock"
if ! flock --nonblock 9; then
  printf 'Another Mish local Nix command is running; use GitHub CI or retry later.\n' >&2
  exit 75
fi

# Both variables also constrain nested Nix and Cargo commands.
export NIX_CONFIG="${NIX_CONFIG:-}
max-jobs = 1
cores = 1"
export CARGO_BUILD_JOBS=1
if [ "$1" = --command ]; then
  shift
  if [ "$#" -eq 0 ]; then exit 64; fi
  task_command=("$@")
else
  task_command=(nix --max-jobs 1 --cores 1 "$@")
fi

# This scope bounds the evaluator/client and subprocesses. Daemon-side builds
# live outside it, so keep one build job and run images/VM gates in GitHub CI.
if ! command -v systemd-run >/dev/null; then
  printf 'A systemd user scope is required for bounded local validation; use GitHub CI on this host.\n' >&2
  exit 69
fi
exec systemd-run --user --scope --quiet \
  --property=MemoryHigh=1536M --property=MemoryMax=2G \
  --property=MemorySwapMax=0 \
  nix develop --max-jobs 1 --cores 1 --accept-flake-config \
  --command "${task_command[@]}"
