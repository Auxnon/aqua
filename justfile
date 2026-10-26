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

# Time the simulation at 2048×2048
bench:
    cd engine && cargo run --release --example bench

# Show the app's log output
logs:
    adb logcat -s aqua:V AndroidRuntime:E

# Remove Gradle, Rust and jniLibs build output
clean:
    ./gradlew clean
    cd engine && cargo clean
    rm -rf app/src/main/jniLibs
