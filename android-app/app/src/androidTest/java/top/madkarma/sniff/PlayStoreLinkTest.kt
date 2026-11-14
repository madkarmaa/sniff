package top.madkarma.sniff

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import top.madkarma.sniff.data.packageIdFromInput
import top.madkarma.sniff.data.packageInputFromSharedText

@RunWith(AndroidJUnit4::class)
class PlayStoreLinkTest {
    @Test
    fun extractsPackageIdFromPlayLinks() {
        listOf(
                "https://play.google.com/store/apps/details?id=com.discord&hl=en",
                "  https://play.google.com/store/apps/details?hl=en&id=com.discord\n",
                "http://play.google.com/store/apps/details/?id=com.discord",
                "market://details?id=com.discord",
            )
            .forEach { assertEquals(it, "com.discord", packageIdFromInput(it)) }
    }

    @Test
    fun resolvesSharedText() {
        assertEquals(
            "com.discord",
            packageInputFromSharedText(
                "Check out Discord on Google Play\nhttps://play.google.com/store/apps/details?id=com.discord&hl=en.",
            ),
        )
        assertEquals("com.discord", packageInputFromSharedText("  com.discord \n"))
        assertEquals(
            "https://example.com/page",
            packageInputFromSharedText("See https://example.com/page)"),
        )
    }

    @Test
    fun leavesOtherInputUnchanged() {
        assertEquals("com.discord", packageIdFromInput(" com.discord "))
        assertEquals(
            "https://example.com/store/apps/details?id=com.discord",
            packageIdFromInput("https://example.com/store/apps/details?id=com.discord"),
        )
        assertEquals(
            "https://play.google.com/store/apps/details",
            packageIdFromInput("https://play.google.com/store/apps/details"),
        )
        assertEquals("mailto:someone", packageIdFromInput("mailto:someone"))
    }
}
