import java.util.Properties

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

// The owner's Firebase project (milestone 2): `android/app/google-services.json`, which git ignores. Without it the app
// builds and runs, and the push service simply never receives a message; see docs/development.md.
val hasFirebaseConfig = file("google-services.json").exists()
if (hasFirebaseConfig) {
    apply(plugin = "com.google.gms.google-services")
}

// Release signing comes from `android/key.properties` (git ignores it), written by the owner or by the release job. A
// build without it is signed with the debug key, which is fine for `flutter build apk --release` on a computer or in CI
// but is never what ships.
val keyProperties = Properties().apply {
    val f = rootProject.file("key.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}

android {
    namespace = "dev.bobix.phone"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        applicationId = "dev.bobix.phone"
        // Android 10: the first version with `BiometricPrompt` Class 3 for every app and a key bound to biometrics.
        minSdk = 29
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    signingConfigs {
        if (keyProperties.isNotEmpty()) {
            create("release") {
                keyAlias = keyProperties.getProperty("keyAlias")
                keyPassword = keyProperties.getProperty("keyPassword")
                storeFile = file(keyProperties.getProperty("storeFile"))
                storePassword = keyProperties.getProperty("storePassword")
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
            // JNA and the UniFFI classes are found by name at run time: R8 must keep them.
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

dependencies {
    // The biometric prompt that guards the signing key (needs a FragmentActivity: see MainActivity).
    implementation("androidx.biometric:biometric:1.1.0")
    // Push: data messages only, no analytics. `firebase_messaging_auto_init_enabled` is off in the manifest.
    implementation(platform("com.google.firebase:firebase-bom:34.0.0"))
    implementation("com.google.firebase:firebase-messaging")
    // UniFFI's Kotlin binding (uniffi/bobix_phone_core) loads libbobix_phone_core.so, the one library the app already
    // ships for flutter_rust_bridge, through JNA.
    implementation("net.java.dev.jna:jna:5.17.0@aar")

    // Unit tests run on this computer. JNA there loads the Rust library built for it (`cargo build` in mobile/rust).
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20250517")
    testImplementation("net.java.dev.jna:jna:5.17.0")
}

tasks.withType<Test>().configureEach {
    systemProperty("jna.library.path", rootProject.file("../rust/target/debug").absolutePath)
    systemProperty("notice.fixtures", file("src/test/resources/notice-fixtures.json").absolutePath)
    systemProperty("app.src.main", file("src/main").absolutePath)
}

flutter {
    source = "../.."
}
