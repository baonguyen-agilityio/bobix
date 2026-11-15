#!/bin/sh
# The push core (mobile/push-core) for the iOS app and its notification service extension.
#
#   push-core.sh build              Xcode's Run Script phase: builds the static library for the SDK being built.
#   push-core.sh bindings           regenerates ios/PushCore/ (the UniFFI Swift file, header and module map).
#   push-core.sh bindings --check   fails when the committed bindings are not what the Rust API says.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
core="$here/push-core"
out="$here/ios/PushCore"
# Xcode's build environment has a minimal PATH.
PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
export PATH

generate() { # $1 = output directory
  (cd "$core" && cargo build --quiet --features cli && \
    cargo run --quiet --features cli --bin uniffi-bindgen -- generate \
      --library "$core/target/debug/libbobix_push.a" --language swift --out-dir "$1")
  mv "$1/bobix_pushFFI.modulemap" "$1/module.modulemap"
}

case "${1:-}" in
  build)
    # Simulator on Apple silicon only (the project excludes x86_64 for the simulator); a device is arm64.
    case "${PLATFORM_NAME:-iphonesimulator}" in
      iphoneos) triple=aarch64-apple-ios ;;
      *) triple=aarch64-apple-ios-sim ;;
    esac
    # The host's build tools must not see the iOS SDK that Xcode puts in the environment.
    unset SDKROOT LIBRARY_PATH
    (cd "$core" && IPHONEOS_DEPLOYMENT_TARGET=18.0 cargo build --quiet --release --target "$triple")
    ;;
  bindings)
    if [ "${2:-}" = "--check" ]; then
      tmp=$(mktemp -d)
      trap 'rm -rf "$tmp"' EXIT
      generate "$tmp"
      diff -r "$tmp" "$out" >/dev/null || { echo "ios/PushCore is out of date: run mobile/tool/push-core.sh bindings" >&2; exit 1; }
    else
      mkdir -p "$out"
      generate "$out"
    fi
    ;;
  *) echo "usage: push-core.sh build | bindings [--check]" >&2; exit 2 ;;
esac
