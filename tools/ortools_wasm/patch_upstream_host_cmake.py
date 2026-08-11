from __future__ import annotations

import argparse
from pathlib import Path


ORIGINAL_BLOCK = """add_custom_command(
  OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/host_tools
  COMMAND ${CMAKE_COMMAND} -E remove_directory build
  COMMAND ${CMAKE_COMMAND} -S. -Bbuild -DCMAKE_BUILD_TYPE=Release -DCMAKE_RUNTIME_OUTPUT_DIRECTORY:PATH=${CMAKE_CURRENT_BINARY_DIR}/host_tools/bin
  COMMAND ${CMAKE_COMMAND} --build build --config Release -v
  WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/host_tools
)

add_custom_target(host_tools
  DEPENDS ${CMAKE_CURRENT_BINARY_DIR}/host_tools
  WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR})"""

PATCHED_BLOCK = """set(HOST_TOOLS_STAMP ${CMAKE_CURRENT_BINARY_DIR}/host_tools.stamp)

add_custom_command(
  OUTPUT ${HOST_TOOLS_STAMP}
  BYPRODUCTS ${CMAKE_CURRENT_BINARY_DIR}/host_tools/bin/protoc
  COMMAND ${CMAKE_COMMAND} -E remove -f ${HOST_TOOLS_STAMP}
  COMMAND ${CMAKE_COMMAND} -E remove_directory build
  COMMAND ${CMAKE_COMMAND} -S. -Bbuild -DCMAKE_BUILD_TYPE=Release -DCMAKE_RUNTIME_OUTPUT_DIRECTORY:PATH=${CMAKE_CURRENT_BINARY_DIR}/host_tools/bin
  COMMAND ${CMAKE_COMMAND} --build build --config Release -v
  COMMAND ${CMAKE_COMMAND} -E touch ${HOST_TOOLS_STAMP}
  WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/host_tools
)

add_custom_target(host_tools
  DEPENDS ${HOST_TOOLS_STAMP}
  WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR})"""


def patch_host_cmake(source_dir: Path) -> Path:
    host_cmake_path = source_dir / "cmake" / "host.cmake"
    content = host_cmake_path.read_text(encoding="utf-8")

    if PATCHED_BLOCK in content:
        return host_cmake_path

    if ORIGINAL_BLOCK not in content:
        raise ValueError(f"unsupported upstream host.cmake format: {host_cmake_path}")

    host_cmake_path.write_text(
        content.replace(ORIGINAL_BLOCK, PATCHED_BLOCK),
        encoding="utf-8",
    )
    return host_cmake_path


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Patch OR-Tools upstream host.cmake for CMake/Ninja cross-build compatibility",
    )
    parser.add_argument("--source-dir", required=True, help="Extracted OR-Tools source directory")
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    patched_path = patch_host_cmake(Path(args.source_dir).resolve())
    print(patched_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
