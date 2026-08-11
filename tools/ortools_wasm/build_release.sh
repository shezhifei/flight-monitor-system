#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
UPSTREAM_MANIFEST="${SCRIPT_DIR}/upstream-manifest.json"
OUTPUT_DIR=""
ARTIFACT_VERSION=""
WORK_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --artifact-version)
      ARTIFACT_VERSION="$2"
      shift 2
      ;;
    --work-dir)
      WORK_DIR="$2"
      shift 2
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

if [[ -z "${OUTPUT_DIR}" ]]; then
  OUTPUT_DIR="${REPO_ROOT}/dist/ortools"
fi

if [[ -z "${ARTIFACT_VERSION}" ]]; then
  ARTIFACT_VERSION="$(python3 -c "import json, pathlib; print(json.loads(pathlib.Path('${UPSTREAM_MANIFEST}').read_text(encoding='utf-8'))['artifact_version'])")"
fi

if [[ -z "${WORK_DIR}" ]]; then
  WORK_DIR="${REPO_ROOT}/.tmp/ortools-wasm"
fi

mkdir -p "${OUTPUT_DIR}" "${WORK_DIR}"
SOURCE_URL="$(python3 -c "import json, pathlib; print(json.loads(pathlib.Path('${UPSTREAM_MANIFEST}').read_text(encoding='utf-8'))['upstream']['source_url'])")"
SOURCE_SHA256="$(python3 -c "import json, pathlib; print(json.loads(pathlib.Path('${UPSTREAM_MANIFEST}').read_text(encoding='utf-8'))['upstream']['source_sha256'])")"
ARCHIVE_PATH="${WORK_DIR}/or-tools-source.tar.gz"
SOURCE_DIR="${WORK_DIR}/source"
BUILD_DIR="${WORK_DIR}/build"
RELEASE_DIR="${OUTPUT_DIR}/${ARTIFACT_VERSION}"

curl -L "${SOURCE_URL}" -o "${ARCHIVE_PATH}"
python3 - <<'PY' "${ARCHIVE_PATH}" "${SOURCE_SHA256}"
import hashlib
import pathlib
import sys

archive_path = pathlib.Path(sys.argv[1])
expected = sys.argv[2].strip().lower()
actual = hashlib.sha256(archive_path.read_bytes()).hexdigest()
if actual != expected:
    raise SystemExit(f"source sha256 mismatch: expected {expected}, got {actual}")
PY

rm -rf "${SOURCE_DIR}" "${BUILD_DIR}" "${RELEASE_DIR}"
mkdir -p "${SOURCE_DIR}" "${BUILD_DIR}" "${RELEASE_DIR}"
tar -xf "${ARCHIVE_PATH}" -C "${SOURCE_DIR}" --strip-components=1
python3 "${SCRIPT_DIR}/patch_upstream_host_cmake.py" --source-dir "${SOURCE_DIR}" >/dev/null

emcmake cmake \
  -S "${SCRIPT_DIR}" \
  -B "${BUILD_DIR}" \
  -GNinja \
  -DCMAKE_BUILD_TYPE=Release \
  -DORTOOLS_SOURCE_DIR="${SOURCE_DIR}"

cmake --build "${BUILD_DIR}" --target dispatch_replan_solver

cp "${BUILD_DIR}/dispatch_replan_solver.js" "${RELEASE_DIR}/dispatch_replan_solver.js"
cp "${BUILD_DIR}/dispatch_replan_solver.wasm" "${RELEASE_DIR}/dispatch_replan_solver.wasm"

mkdir -p "${RELEASE_DIR}/LICENSES"
cp "${SOURCE_DIR}/LICENSE" "${RELEASE_DIR}/LICENSES/or-tools-LICENSE"
cp "${SOURCE_DIR}/Dependencies.txt" "${RELEASE_DIR}/LICENSES/Dependencies.txt"
find "${BUILD_DIR}" -path '*/_deps/*' \( -iname 'LICENSE*' -o -iname 'COPYING*' -o -iname 'NOTICE*' \) -type f -exec cp --parents {} "${RELEASE_DIR}/LICENSES" \;

tar -czf "${RELEASE_DIR}/LICENSES.tar.gz" -C "${RELEASE_DIR}" LICENSES
python3 "${SCRIPT_DIR}/generate_release_manifest.py" --output-dir "${RELEASE_DIR}" --upstream-manifest "${UPSTREAM_MANIFEST}" >/dev/null
node "${SCRIPT_DIR}/run_fixture_tests.mjs" --js "${RELEASE_DIR}/dispatch_replan_solver.js" --wasm "${RELEASE_DIR}/dispatch_replan_solver.wasm"
tar -czf "${OUTPUT_DIR}/frontend-ortools-cpsat-official-${ARTIFACT_VERSION}.tar.gz" -C "${OUTPUT_DIR}" "${ARTIFACT_VERSION}"
