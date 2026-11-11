#!/usr/bin/env bash
# Build a private, pinned Aquamarine for greeter-only KMS preservation.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
version=0.16.0
revision=b77d67648336223a6c8fb88844ae316e7b72b574
artifact="$ROOT/tools/greeter/target/handoff"
if [[ "$(pkg-config --modversion aquamarine)" != "$version" ]]; then
  printf 'Greeter handoff patch supports Aquamarine %s; using the stock backend.\n' "$version" >&2
  exit 0
fi
mkdir -p "$(dirname -- "$artifact")"
work="$(mktemp -d "${artifact}.build.XXXXXX")"
trap 'rm -rf -- "$work"' EXIT
# Verify the immutable revision even if the upstream tag is later moved.
git clone --quiet --depth 1 --branch "v$version" https://github.com/hyprwm/aquamarine.git "$work/source"
[[ "$(git -C "$work/source" rev-parse HEAD)" == "$revision" ]]
git -C "$work/source" apply "$ROOT/tools/greeter/patches/aquamarine-$version-preserve-greeter-outputs.patch"
cmake -S "$work/source" -B "$work/build" -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build "$work/build" --target aquamarine attachments output commitThread -j "${GREETER_BUILD_JOBS:-4}"
ctest --test-dir "$work/build" -R '^(attachments|output|commitThread)$' --output-on-failure
# This harness calls the real library against fake KMS; it never opens a GPU/seat.
read -r -a flags <<< "$(pkg-config --cflags --libs aquamarine pixman-1 hyprutils libdrm)"
c++ -std=c++23 -Wall -Wextra "$ROOT/tests/greeter_drm_handoff.cpp" \
  -I"$work/source/include" -L"$work/build" -Wl,-rpath,"$work/build" \
  "${flags[@]}" -laquamarine -o "$work/build/handoff-test"
"$work/build/handoff-test"
mkdir "$work/result"
install -m 755 "$work/build/libaquamarine.so.$version" "$work/result/libaquamarine.so.15"
# Catch compositor, ABI-library and transitive dependency upgrades. A mismatch
# makes the root-owned launcher fall back to the stock library at the next login.
{
  printf '%s\n' /usr/bin/Hyprland /usr/lib/libaquamarine.so.15
  ldd "$work/build/libaquamarine.so.15" | awk '/=> \// {print $3}'
} | sort -u | while IFS= read -r path; do
  sha256sum "$path"
done > "$work/result/system.sha256"
printf '%s\n' "$revision" > "$work/result/source-revision"
mkdir -p "$artifact"
install -m 755 "$work/result/libaquamarine.so.15" "$artifact/libaquamarine.so.15"
install -m 644 "$work/result/system.sha256" "$work/result/source-revision" "$artifact/"
printf 'Built and tested private greeter backend: %s\n' "$artifact"
