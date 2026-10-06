plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "top.madkarma.sniff"
    compileSdk = 37

    defaultConfig {
        applicationId = "top.madkarma.sniff"
        minSdk = 31
        targetSdk = 37
        versionCode = 3
        versionName = "1.0.1"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildFeatures { compose = true }

    signingConfigs.configureEach {
        enableV1Signing = false
        enableV2Signing = true
        enableV3Signing = true
        enableV4Signing = false
    }

    lint { warningsAsErrors = true }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    packaging {
        resources.excludes += setOf(
            "META-INF/DEPENDENCIES", "META-INF/LICENSE*", "META-INF/NOTICE*"
        )
    }
}

kotlin { compilerOptions { allWarningsAsErrors = true } }

dependencies {
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.core.ktx)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.okhttp)
    implementation(libs.arsclib)
    implementation(libs.apksig)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
}
