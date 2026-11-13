package top.madkarma.sniff

import android.content.Context
import androidx.core.content.pm.PackageInfoCompat
import androidx.test.platform.app.InstrumentationRegistry
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.atomic.AtomicInteger

/** Shared fixtures of the download pipeline tests: a canned API response and a fake network. */
abstract class PipelineTestBase {
    protected val context: Context get() = InstrumentationRegistry.getInstrumentation().targetContext

    protected val original get() = File(context.applicationInfo.sourceDir).readBytes()
    protected val installedVersion
        get() = PackageInfoCompat.getLongVersionCode(
            context.packageManager.getPackageInfo(context.packageName, 0)
        )

    /** API response describing this app as a standalone APK with expansion and dex files. */
    protected fun standaloneResponse(versionCode: Long): String {
        val data = JSONObject().put(
            "item", JSONObject().put("title", "Test fixture").put(
                "details", JSONObject().put(
                    "app_details",
                    JSONObject().put("package_name", context.packageName)
                        .put("version_code", versionCode)
                )
            )
        ).put("main_apk_url", "https://fixture.invalid/base.apk").put("splits", JSONArray()).put(
            "additional_files", JSONArray().put(
                JSONObject().put("filename", "main.1.${context.packageName}.obb")
                    .put("download_url", "https://fixture.invalid/main.obb")
            )
        ).put("dex_metadata_url", "https://fixture.invalid/base.dm")
        return JSONObject().put("success", true).put("data", data).toString()
    }

    /** Serves [response] for API calls and fixture bytes for downloads, counting the downloads. */
    protected fun fixtureClient(response: () -> String, downloads: AtomicInteger): OkHttpClient {
        val apk = original
        return OkHttpClient.Builder().addInterceptor { chain ->
            val bytes = when {
                chain.request().url.host == "sniff.madkarma.top" -> response().toByteArray()
                else -> {
                    downloads.incrementAndGet()
                    if (chain.request().url.encodedPath == "/base.apk") apk
                    else "extra fixture data".toByteArray()
                }
            }
            Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(200)
                .message("Fixture").body(bytes.toResponseBody()).build()
        }.build()
    }
}
