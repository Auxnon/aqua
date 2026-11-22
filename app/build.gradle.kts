import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

// ---------------------------------------------------------------- version
//
// Releases are git tags, vMAJOR.MINOR[.PATCH] (`just tag 0.2`). The version name
// is `git describe`: "0.1" on the tag itself, "0.1-3-gabc1234" three commits
// after it, with "-dirty" for uncommitted changes. The version code grows with
// every tag and every commit after one, so each build installs over the last.

fun git(vararg args: String): String? = runCatching {
    providers.exec { commandLine("git", *args); isIgnoreExitValue = true }.standardOutput.asText.get().trim().ifEmpty { null }
}.getOrNull()

val described: String? = git("describe", "--tags", "--match", "v[0-9]*", "--dirty")
val lastTag: String? = git("describe", "--tags", "--abbrev=0", "--match", "v[0-9]*")
val aquaVersionName: String = described?.removePrefix("v") ?: "0.0-dev"
val aquaVersionCode: Int = run {
    val parts = lastTag?.removePrefix("v")?.split(".")?.map { it.toIntOrNull() ?: 0 } ?: emptyList()
    val base = parts.getOrElse(0) { 0 } * 1_000_000 + parts.getOrElse(1) { 0 } * 10_000 + parts.getOrElse(2) { 0 } * 100
    val since = lastTag?.let { git("rev-list", "--count", "$it..HEAD")?.toIntOrNull() } ?: git("rev-list", "--count", "HEAD")?.toIntOrNull() ?: 0
    (base + since.coerceAtMost(99)).coerceAtLeast(1)
}

// A release key, if there is one: keystore.properties (not in git) with
// storeFile, storePassword, keyAlias and keyPassword. Without it, release
// builds are signed with the debug key so they still install.
val keystoreProps = Properties().apply {
    rootProject.file("keystore.properties").takeIf { it.exists() }?.inputStream()?.use { load(it) }
}

android {
    namespace = "app.aqua"
    compileSdk = 37

    defaultConfig {
        applicationId = "app.aqua"
        minSdk = 29
        targetSdk = 36
        versionCode = aquaVersionCode
        versionName = aquaVersionName
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    signingConfigs {
        if (keystoreProps.getProperty("storeFile") != null) {
            create("release") {
                storeFile = rootProject.file(keystoreProps.getProperty("storeFile"))
                storePassword = keystoreProps.getProperty("storePassword")
                keyAlias = keystoreProps.getProperty("keyAlias")
                keyPassword = keystoreProps.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // The release key when there is one, else the debug key so
            // `assembleRelease` still makes an installable APK.
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }

    packaging {
        jniLibs.useLegacyPackaging = false
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2026.09.00")
    implementation(composeBom)
    implementation("androidx.core:core-ktx:1.19.1")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.11.0")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended:1.7.8")
    debugImplementation("androidx.compose.ui:ui-tooling")
}

// ---------------------------------------------------------------- Rust engine

val sdkDir: String = run {
    val p = Properties()
    rootProject.file("local.properties").takeIf { it.exists() }?.inputStream()?.use { p.load(it) }
    p.getProperty("sdk.dir") ?: System.getenv("ANDROID_HOME") ?: "${System.getProperty("user.home")}/Android/Sdk"
}

val cargo: String = listOf("${System.getProperty("user.home")}/.cargo/bin/cargo", "/usr/local/bin/cargo")
    .firstOrNull { file(it).exists() } ?: "cargo"

val buildRust by tasks.registering(Exec::class) {
    description = "Builds the Rust engine (engine/) for Android with cargo-ndk."
    val engineDir = rootProject.file("engine")
    val out = file("src/main/jniLibs")
    workingDir = engineDir
    inputs.dir(engineDir.resolve("src"))
    inputs.file(engineDir.resolve("Cargo.toml"))
    outputs.dir(out)
    val ndkRoot = file("$sdkDir/ndk").listFiles()?.filter { it.isDirectory }?.maxByOrNull { it.name }
    if (ndkRoot != null) environment("ANDROID_NDK_HOME", ndkRoot.absolutePath)
    environment("PATH", "${System.getProperty("user.home")}/.cargo/bin:${System.getenv("PATH")}")
    commandLine(cargo, "ndk", "-t", "arm64-v8a", "-t", "x86_64", "--platform", "29", "-o", out.absolutePath, "build", "--release")
}

tasks.named("preBuild") { dependsOn(buildRust) }
