buildscript {
    // Override AGP's older embedded KGP while retaining built-in Kotlin.
    dependencies { classpath(libs.kotlin.gradle.plugin) }
}

plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.compose) apply false
}
