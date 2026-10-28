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

# Show the app's log output
logs:
    adb logcat -s aqua:V AndroidRuntime:E

# Remove Gradle, Rust and jniLibs build output
clean:
    ./gradlew clean
    cd engine && cargo clean
    rm -rf app/src/main/jniLibs
    cd web && cargo clean
    rm -rf web/www/pkg
