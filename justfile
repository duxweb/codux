set shell := ["sh", "-eu", "-c"]

default:
    just --list

desktop *args:
    sh tools/run-desktop-dev.sh {{args}}

agent *args:
    cargo run -p codux-agent -- {{args}}

mobile *args:
    cd apps/mobile && \
    set -- {{args}}; \
    platform="${1:-android}"; \
    case "$platform" in \
      android|ios) shift || true ;; \
      *) platform="" ;; \
    esac; \
    if [ -n "$platform" ]; then \
        if [ "$platform" = "ios" ]; then \
            ./scripts/configure-ios-local-signing.sh; \
        fi; \
        device_id="$(flutter devices --machine | ruby -rjson -e 'platform = ARGV[0]; devices = JSON.parse(STDIN.read); device = devices.find { |item| item["isSupported"] && item["targetPlatform"].to_s.start_with?(platform) }; print(device ? device["id"] : "")' "$platform")"; \
        if [ -n "$device_id" ]; then \
            echo "Using $platform device: $device_id"; \
            if [ "$platform" = "ios" ]; then \
                mode="debug"; \
                for arg in "$@"; do \
                    case "$arg" in \
                      --release) mode="release" ;; \
                      --profile) mode="profile" ;; \
                      --debug) mode="debug" ;; \
                    esac; \
                done; \
                if [ "$mode" = "debug" ]; then \
                    flutter build ios --debug; \
                    mkdir -p build/ios/iphoneos; \
                    rm -rf build/ios/iphoneos/Runner.app; \
                    cp -R build/ios/Debug-iphoneos/Runner.app build/ios/iphoneos/Runner.app; \
                    flutter run -d "$device_id" --no-build "$@"; \
                else \
                    flutter run -d "$device_id" "$@"; \
                fi; \
            else \
                ./plugin/codux_protocol_ffi/scripts/build-android.sh; \
                adb shell am force-stop com.duxweb.codux.dev >/dev/null 2>&1 || true; \
                adb shell am force-stop com.duxweb.codux >/dev/null 2>&1 || true; \
                flutter run -d "$device_id" "$@"; \
            fi; \
        else \
            echo "No $platform device found. Falling back to flutter run."; \
            flutter run "$@"; \
        fi; \
    else \
        flutter run "$@"; \
    fi

check:
    cargo check --workspace
    cd apps/mobile && flutter analyze

test:
    cargo test --workspace
    cd apps/mobile && flutter test

ffi:
    cargo build -p codux-protocol-ffi

smoke:
    cargo run -p codux-agent -- --pty-smoke
    cargo run -p codux-agent -- --transport-smoke
