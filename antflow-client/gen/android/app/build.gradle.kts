import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

val keystoreProperties = Properties().apply {
    val propFile = rootProject.file("keystore.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 36
    namespace = "com.antflow.app"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.antflow.app"
        minSdk = 24
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    
    signingConfigs {
        create("releaseConfig") {
            if (keystoreProperties.containsKey("storeFile")) {
                val path = keystoreProperties.getProperty("storeFile")
                storeFile = if (path.startsWith("C:\\") || path.startsWith("/") || path.contains(":\\")) file(path) else rootProject.file(path)
                storePassword = keystoreProperties.getProperty("storePassword", "")
                keyAlias = keystoreProperties.getProperty("keyAlias", "upload")
                keyPassword = keystoreProperties.getProperty("keyPassword", "")
            } else {
                val keystorePath = System.getenv("ANDROID_KEYSTORE_FILE")
                    ?: System.getenv("KEYSTORE_PATH")
                    ?: "C:\\Users\\Jeison\\upload-keystore.jks"
                
                val hasKeystore = keystorePath.isNotEmpty() && file(keystorePath).exists()
                
                if (hasKeystore) {
                    storeFile = file(keystorePath)
                    storePassword = System.getenv("ANDROID_KEYSTORE_PASS")
                        ?: System.getenv("KEYSTORE_PASSWORD")
                        ?: ""
                    keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                        ?: System.getenv("KEY_ALIAS")
                        ?: "upload"
                    keyPassword = System.getenv("ANDROID_KEY_PASS")
                        ?: System.getenv("KEY_PASSWORD")
                        ?: ""
                } else {
                    val debugConfig = signingConfigs.getByName("debug")
                    storeFile = debugConfig.storeFile
                    storePassword = debugConfig.storePassword
                    keyAlias = debugConfig.keyAlias
                    keyPassword = debugConfig.keyPassword
                }
            }
        }
    }

    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            val hasKeystoreProp = keystoreProperties.containsKey("storeFile") && 
                run {
                    val path = keystoreProperties.getProperty("storeFile")
                    val f = if (path.startsWith("C:\\") || path.startsWith("/") || path.contains(":\\")) file(path) else rootProject.file(path)
                    f.exists()
                }
            val keystorePath = System.getenv("ANDROID_KEYSTORE_FILE")
                ?: System.getenv("KEYSTORE_PATH")
                ?: "C:\\Users\\Jeison\\upload-keystore.jks"
            val storePass = System.getenv("ANDROID_KEYSTORE_PASS")
                ?: System.getenv("KEYSTORE_PASSWORD")
                ?: ""
            
            if (hasKeystoreProp || (storePass.isNotEmpty() && file(keystorePath).exists())) {
                signingConfig = signingConfigs.getByName("releaseConfig")
            } else {
                signingConfig = signingConfigs.getByName("debug")
            }
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")