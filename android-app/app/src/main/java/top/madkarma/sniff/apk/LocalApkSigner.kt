package top.madkarma.sniff.apk

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import com.android.apksig.ApkSigner
import com.android.apksig.KeyConfig
import java.io.File
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.cert.X509Certificate
import java.util.Date
import javax.security.auth.x500.X500Principal

private const val KEYSTORE_PROVIDER = "AndroidKeyStore"
private const val SIGNING_KEY_ALIAS = "sniff-apk-signing"
private const val CERTIFICATE_EXPIRY_MILLIS = 4_102_444_800_000L

internal object LocalApkSigner {
    fun sign(input: File, output: File) {
        ApkSigner.Builder(listOf(signerConfig())).setInputApk(input).setOutputApk(output)
            .setV1SigningEnabled(false).setV2SigningEnabled(true).setV3SigningEnabled(true)
            .setV4SigningEnabled(false).build().sign()
    }

    private fun signerConfig(): ApkSigner.SignerConfig {
        val store = KeyStore.getInstance(KEYSTORE_PROVIDER).apply { load(null) }
        if (!store.containsAlias(SIGNING_KEY_ALIAS)) createSigningKey()

        val privateKey = store.getKey(SIGNING_KEY_ALIAS, null) as PrivateKey
        val certificate = store.getCertificate(SIGNING_KEY_ALIAS) as X509Certificate

        return ApkSigner.SignerConfig.Builder(
            "sniff", KeyConfig.Jca(privateKey), listOf(certificate)
        ).build()
    }

    private fun createSigningKey() {
        val parameters = KeyGenParameterSpec.Builder(
            SIGNING_KEY_ALIAS,
            KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY,
        ).setKeySize(2048).setDigests(
            KeyProperties.DIGEST_SHA256,
            KeyProperties.DIGEST_SHA512,
        ).setSignaturePaddings(KeyProperties.SIGNATURE_PADDING_RSA_PKCS1)
            .setCertificateSubject(X500Principal("CN=Sniff"))
            .setCertificateNotAfter(Date(CERTIFICATE_EXPIRY_MILLIS)).build()

        KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_RSA, KEYSTORE_PROVIDER).apply {
            initialize(parameters)
            generateKeyPair()
        }
    }
}
