# List recipes
default:
    @just --list

# Build the debug APK and install it on the connected device
install:
    ./gradlew installDebug

# Build the minified release APK and install it on the connected device
install-release:
    ./gradlew installRelease

# Install and launch the app
run: install
    adb shell am start -n app.aqua/.MainActivity

# Build the debug APK without installing
build:
    ./gradlew assembleDebug

# Build the release APK
release:
    ./gradlew assembleRelease

# Run the engine's unit tests on the desktop GPU
test:
    cd engine && cargo test --release --lib

# Paint every brush and effect headlessly and write PNGs to `out`
paint-test out="/tmp/aqua_out":
    cd engine && cargo run --release --example paint_test -- {{out}}

# Resting brush, ink splatter and calm vs wild floods, written as PNGs to `out`
brush-test out="/tmp/aqua_brush":
    cd engine && cargo run --release --example brush_test -- {{out}}

# Time the simulation at 2048×2048
bench:
    cd engine && cargo run --release --example bench

# Build the web app (engine to wasm with wasm-pack) into web/www
web-build:
    cd web && wasm-pack build --release --target web --out-dir www/pkg --no-typescript

# Build and serve the web app at http://localhost:8080 (WebGPU needs localhost or https)
web port="8080": web-build
    cd web/www && python3 -m http.server {{port}} --bind 127.0.0.1

# Build and run the desktop app (Linux, macOS)
desktop *args:
    cd desktop && cargo run --release -- {{args}}

# Build the desktop app as an AppImage (desktop/target/Aqua-<arch>.AppImage)
appimage:
    desktop/package/appimage.sh

# Build Aqua.app and Aqua.dmg (on a Mac)
macos-app:
    desktop/package/macos.sh

# Run the sync daemon in the foreground (paintings land in the desktop gallery)
sync-daemon *args:
    cd sync && cargo run --release -- {{args}}

# Install the sync daemon as a systemd user service and start it (Linux)
sync-install:
    cargo install --path sync
    mkdir -p ~/.config/systemd/user
    cp sync/aqua-sync.service ~/.config/systemd/user/
    systemctl --user daemon-reload
    systemctl --user enable --now aqua-sync
    @echo "aqua-sync is listening on port 7717. If a firewall is on, open it: sudo firewall-cmd --add-port=7717/tcp --permanent && sudo firewall-cmd --reload"

# Show the app's log output
logs:
    adb logcat -s aqua:V AndroidRuntime:E

# Remove Gradle, Rust and jniLibs build output
clean:
    ./gradlew clean
    cd engine && cargo clean
    rm -rf app/src/main/jniLibs
    cd web && cargo clean
    cd desktop && cargo clean
    cd sync && cargo clean
    rm -rf web/www/pkg
