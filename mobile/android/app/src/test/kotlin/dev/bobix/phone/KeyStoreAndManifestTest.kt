package dev.bobix.phone

import androidx.biometric.BiometricPrompt
import dev.bobix.phone.keys.KeyStore
import java.io.File
import java.security.KeyPairGenerator
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class KeyStoreAndManifestTest {
    private fun src(path: String) = File(System.getProperty("app.src.main")!!, path).readText()

    @Test
    fun aPublicKeyIsSixtyFiveBytesWhateverTheCoordinatesLookLike() {
        val gen = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }
        repeat(300) {
            val bytes = KeyStore.x963(gen.generateKeyPair().public as ECPublicKey)
            assertEquals(65, bytes.size)
            assertEquals(4, bytes[0].toInt())
        }
    }

    @Test
    fun biometricErrorsMeanWhatTheKeyInterfaceSays() {
        fun name(code: Int) = KeyStore.outcomeOfError(code, "m")::class.simpleName
        assertEquals("Cancelled", name(BiometricPrompt.ERROR_USER_CANCELED))
        assertEquals("Cancelled", name(BiometricPrompt.ERROR_NEGATIVE_BUTTON))
        assertEquals("LockedOut", name(BiometricPrompt.ERROR_LOCKOUT))
        assertEquals("LockedOut", name(BiometricPrompt.ERROR_LOCKOUT_PERMANENT))
        assertEquals("Unavailable", name(BiometricPrompt.ERROR_NO_BIOMETRICS))
        assertEquals("Unavailable", name(BiometricPrompt.ERROR_HW_NOT_PRESENT))
        assertEquals("Failed", name(BiometricPrompt.ERROR_TIMEOUT))
    }

    @Test
    fun noBackupOfAnythingTheAppKeeps() {
        val manifest = src("AndroidManifest.xml")
        assertTrue(manifest.contains("android:allowBackup=\"false\""))
        assertTrue(manifest.contains("@xml/data_extraction_rules") && manifest.contains("@xml/backup_rules"))
        for (rules in listOf("res/xml/data_extraction_rules.xml", "res/xml/backup_rules.xml")) {
            val text = src(rules)
            for (domain in listOf("root", "file", "database", "sharedpref", "external")) {
                assertTrue("$rules excludes $domain", text.contains("<exclude domain=\"$domain\""))
            }
            assertFalse("$rules includes nothing", text.contains("<include"))
        }
    }

    @Test
    fun theServiceIsPrivateAndFirebaseStaysOffUntilAsked() {
        val manifest = src("AndroidManifest.xml")
        assertTrue(Regex("""<service\s+android:name="\.push\.PushService"\s+android:exported="false"""").containsMatchIn(manifest))
        assertTrue(manifest.contains("firebase_messaging_auto_init_enabled\" android:value=\"false\""))
        assertTrue(manifest.contains("firebase_analytics_collection_enabled\" android:value=\"false\""))
        assertFalse("no plain-HTTP exception for the whole app", manifest.contains("usesCleartextTraffic"))
    }

    @Test
    fun theDetailsOfANoticeAreNeverTheOnlyThingOnTheLockScreen() {
        // The poster must give every notification a public version and private visibility.
        val poster = src("kotlin/dev/bobix/phone/push/NoticePoster.kt")
        assertTrue(poster.contains("VISIBILITY_PRIVATE") && poster.contains("setPublicVersion(generic)"))
    }
}
