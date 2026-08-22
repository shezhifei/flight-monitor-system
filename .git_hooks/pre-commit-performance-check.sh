#!/usr/bin/env bash
# Reject committed performance knobs that exceed the documented safe range.
set -eu

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

warn() {
  echo "WARNING: $*" >&2
}

check_pool_size() {
  local file="$1"
  local line size
  while IFS= read -r line; do
    case "$line" in
      DB_POOL_SIZE=*|DB_POOL_MAX_CONNECTIONS=*)
        size="${line#*=}"
        size="${size%%[[:space:]]*}"
        if [[ "$size" =~ ^[0-9]+$ ]] && (( size > 64 )); then
          fail "${file}: ${line%%=*}=$size exceeds maximum recommended value of 64"
        fi
        ;;
    esac
  done < "$file"
}

check_actix_workers() {
  local file="$1"
  local line workers cpus
  cpus="$(nproc 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo 4)"
  while IFS= read -r line; do
    case "$line" in
      ACTIX_WORKERS=*)
        workers="${line#*=}"
        workers="${workers%%[[:space:]]*}"
        if [[ "$workers" =~ ^[0-9]+$ ]] && (( workers > cpus * 3 )); then
          warn "${file}: ACTIX_WORKERS=$workers may be excessive given $cpus CPUs"
        fi
        ;;
    esac
  done < "$file"
}

scan_file() {
  local file="$1"
  [[ -f "$file" ]] || return 0
  check_pool_size "$file"
  check_actix_workers "$file"
}

root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$root"

scan_file ".env"
scan_file ".env.example"
scan_file "deploy/docker/.env.edge.example"

if command -v git >/dev/null 2>&1; then
  while IFS= read -r staged; do
    [[ -n "$staged" ]] || continue
    scan_file "$staged"
  done < <(git diff --cached --name-only --diff-filter=ACMR)
fi

echo "Performance config check passed."
