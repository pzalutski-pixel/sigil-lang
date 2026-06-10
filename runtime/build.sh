#!/usr/bin/env bash
#
# Sigil Runtime build for POSIX (Linux / macOS).
# Windows uses build.bat (MSVC) or clang-cl; this is the cc/llvm-ar equivalent.
#
# The C sources are already cross-platform (platform.h abstracts threads,
# mutexes, condition variables, sockets and sleep; file/console/time each have
# POSIX #else branches). This just compiles them with the system C compiler and
# archives them into the same lib/<mode>/sigil_runtime.lib path the examples
# link against.
#
# Honors SIGIL_BUILD_MODE (debug|release, default release), CC (default cc) and
# AR (default llvm-ar — it ships with LLVM, which the compiler already needs).

set -euo pipefail
cd "$(dirname "$0")"

MODE="${SIGIL_BUILD_MODE:-release}"
CC="${CC:-cc}"
AR="${AR:-llvm-ar}"

case "$MODE" in
  debug)   CFLAGS="-c -g -O0 -W -Wall -Iinclude" ;;
  release) CFLAGS="-c -O2 -W -Wall -Iinclude" ;;
  *) echo "Invalid SIGIL_BUILD_MODE=$MODE (must be debug or release)" >&2; exit 1 ;;
esac

# Explicit list (not a glob) so a stray probe file can't sneak into the archive.
SRCS="runtime task scheduler worker channel net file console time memory os"

mkdir -p build
objs=""
for s in $SRCS; do
  echo "  CC  src/$s.c"
  $CC $CFLAGS "src/$s.c" -o "build/$s.o"
  objs="$objs build/$s.o"
done

out="../lib/$MODE/sigil_runtime.lib"
mkdir -p "../lib/$MODE"
rm -f "$out"
$AR rcs "$out" $objs
echo "Built $out"
