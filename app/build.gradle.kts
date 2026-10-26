import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "app.aqua"
    compileSdk = 37

    defaultConfig {
        applicationId = "app.aqua"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Sign with the debug key so `assembleRelease` produces an installable APK out of the box.
            signingConfig = signingConfigs.getByName("debug")
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
