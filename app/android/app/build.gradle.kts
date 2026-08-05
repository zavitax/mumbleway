import java.util.Properties

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

// Release signing material, supplied out of band so no key or password is ever
// committed. CI writes this file from encrypted secrets; a developer building
// locally simply does not have it and gets a debug-signed build instead.
val keystoreProperties = Properties().apply {
    val f = rootProject.file("key.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}
/// A signing value, from the environment first and key.properties second.
///
/// The environment is how a build server should pass a password. A properties
/// file cannot carry one safely: it has to be written by a shell, where an
/// unquoted heredoc expands `$` and backticks, and it is then read by Java's
/// Properties parser, where `\` is an escape character. A password containing
/// any of those three arrives as something else, and the failure — "Get Key
/// failed: Given final block not properly padded" — names neither the shell
/// nor the parser, so it reads as a wrong password that is in fact correct
/// everywhere it was typed.
///
/// key.properties still works, for a developer signing a local release.
fun signingValue(variable: String, property: String): String? =
    System.getenv(variable)?.takeIf { it.isNotEmpty() }
        ?: keystoreProperties.getProperty(property)

val hasReleaseKey = signingValue("ANDROID_KEYSTORE_PATH", "storeFile") != null

android {
    namespace = "com.mumbleway.mumbleway"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "com.mumbleway.mumbleway"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        // cpal's Android backend uses ndk::audio (AAudio), which requires API 26.
        minSdk = maxOf(flutter.minSdkVersion, 26)
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    signingConfigs {
        if (hasReleaseKey) {
            create("release") {
                // Resolved from the root project, which is where
                // key.properties itself lives, so a bare filename in that file
                // means what anybody writing it would expect: the keystore
                // beside it. `file()` here would resolve against this module
                // instead — android/app rather than android — and the build
                // then fails looking for the keystore one directory deeper
                // than anyone put it.
                storeFile =
                    rootProject.file(signingValue("ANDROID_KEYSTORE_PATH", "storeFile")!!)
                storePassword = signingValue("ANDROID_KEYSTORE_PASSWORD", "storePassword")
                keyAlias = signingValue("ANDROID_KEY_ALIAS", "keyAlias")
                keyPassword = signingValue("ANDROID_KEY_PASSWORD", "keyPassword")
            }
        }
    }

    buildTypes {
        release {
            // Debug keys when no upload key is configured, so `flutter run
            // --release` still works for a developer who has none. Play will
            // reject a debug-signed bundle, which is the correct outcome: a
            // build that cannot be published should not look publishable.
            signingConfig = if (hasReleaseKey) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}

// No barcode dependency is declared here, deliberately. mobile_scanner 7
// bundles the ML Kit model itself unless
//
//   dev.steenbakker.mobile_scanner.useUnbundled=true
//
// is set in android/gradle.properties, and it is not set. Only the plugin gets
// to make that choice, so declaring anything from this module is at best noise.
//
// This module did declare `com.google.mlkit:barcode-scanning:17.3.0` for one
// release, to fix a phone that reported
//
//   MobileScannerErrorCode.genericError
//
// with nothing attached, while its camera opened perfectly in every other app.
// That change did not help. The reason is visible in
//
//   ./gradlew :app:dependencies --configuration releaseRuntimeClasspath
//
// which shows `com.google.mlkit:barcode-scanning` depending on
// `com.google.android.gms:play-services-mlkit-barcode-scanning` — the bundled
// artifact is a superset of the unbundled one, not an alternative to it. So
// the model was already in that APK, and "the model was never downloaded" does
// not explain the failure. Whatever does, it is not that; do not re-derive it
// from the dependency list.
