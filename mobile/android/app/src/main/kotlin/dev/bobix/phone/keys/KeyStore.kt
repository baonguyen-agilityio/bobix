package dev.bobix.phone.keys

import android.content.Context
import android.content.pm.ApplicationInfo
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import dev.bobix.phone.R
import java.io.File
import java.security.GeneralSecurityException
import java.security.KeyPairGenerator
import java.security.KeyStore as AndroidKeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The three keys of the app.
 *
 * - `auth` and `box` are secrets the Rust core makes. Each is a file in the app's no-backup folder, encrypted with an
 *   AES key that lives in the Android Keystore (never exportable, no biometrics, usable after the first unlock), so the
 *   push service can read them without the app being open. Nothing here is ever backed up or synced.
 * - `sign` is an ECDSA P-256 key made in the Keystore (in StrongBox where the phone has it), never exportable, used only
 *   after a Class 3 (strong) biometric check on every signature, with no passcode fallback; it stops working when the
 *   enrolled biometrics change. The app only ever gets signatures and the public key.
 *
 * Keystore entries and files can outlive an install on some phones, so the first launch after an install (the marker file
 * is missing) erases every key first.
 */
class KeyStore(
    private val context: Context,
    /** A Keystore key with no biometric requirement, so tests can sign without a person. Debug builds only. */
    private val software: Boolean = false,
    private val dir: File = File(context.noBackupFilesDir, "keys"),
) {
    class KeyError(message: String, cause: Throwable? = null) : Exception(message, cause)

    sealed interface SignOutcome {
        class Signed(val signature: ByteArray) : SignOutcome
        data object Cancelled : SignOutcome
        /** Too many failed matches: the key is fine, the person can unlock with the passcode and try again. */
        data object LockedOut : SignOutcome
        /** Strong biometrics cannot be used right now (none enrolled, turned off). The key is not lost. */
        data object Unavailable : SignOutcome
        /** The key is really gone or unusable (the enrolled biometrics changed): the phone must pair again. */
        data object Invalidated : SignOutcome
        class Failed(val message: String) : SignOutcome
    }

    private val markerFile = File(dir, MARKER)
    private var prompting = false

    init {
        if (software && !isDebuggable(context)) throw KeyError("the software key is not available in a release build")
        eraseIfFreshInstall()
    }

    // MARK: install marker

    private fun eraseIfFreshInstall() {
        if (markerFile.exists()) return
        eraseAll()
        dir.mkdirs()
        if (!markerFile.createNewFile()) throw KeyError("could not write the install marker")
    }

    // MARK: auth and box

    private fun fileFor(name: String): File {
        if (!NAME.matches(name)) throw KeyError("a secret name is letters, digits, - and _ only")
        return File(dir, "$name.secret")
    }

    private fun wrapKey(create: Boolean): SecretKey? {
        val ks = keystore()
        (ks.getKey(WRAP_ALIAS, null) as? SecretKey)?.let { return it }
        if (!create) return null
        val spec = KeyGenParameterSpec.Builder(WRAP_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, PROVIDER).apply { init(spec) }.generateKey()
    }

    fun putSecret(name: String, data: ByteArray) {
        val file = fileFor(name)
        try {
            val cipher = Cipher.getInstance(AES).apply { init(Cipher.ENCRYPT_MODE, wrapKey(create = true)) }
            cipher.updateAAD(name.toByteArray())
            val sealed = cipher.iv + cipher.doFinal(data)
            dir.mkdirs()
            val tmp = File(dir, "$name.tmp")
            tmp.writeBytes(sealed)
            if (!tmp.renameTo(file)) throw KeyError("could not store the secret")
        } catch (e: GeneralSecurityException) {
            throw KeyError("keystore write failed", e)
        }
    }

    /** Null when there is no such secret, or it cannot be opened (its wrapping key is gone). */
    fun secret(name: String): ByteArray? {
        val file = fileFor(name)
        if (!file.exists()) return null
        return try {
            val raw = file.readBytes()
            val key = wrapKey(create = false) ?: return null
            val cipher = Cipher.getInstance(AES).apply {
                init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(128, raw, 0, IV_BYTES))
                updateAAD(name.toByteArray())
            }
            cipher.doFinal(raw, IV_BYTES, raw.size - IV_BYTES)
        } catch (e: GeneralSecurityException) {
            null
        } catch (e: IllegalArgumentException) {
            null
        }
    }

    fun deleteSecret(name: String) {
        fileFor(name).delete()
    }

    // MARK: sign

    private fun keystore(): AndroidKeyStore = AndroidKeyStore.getInstance(PROVIDER).apply { load(null) }

    /** Makes the signing key (replacing any earlier one) and returns its public key as an uncompressed X9.63 point. */
    fun createSigningKey(): ByteArray {
        deleteSigningKey()
        try {
            if (software) generate(strongBox = false, biometric = false)
            else try {
                generate(strongBox = true, biometric = true)
            } catch (e: StrongBoxUnavailableException) {
                generate(strongBox = false, biometric = true)
            }
        } catch (e: GeneralSecurityException) {
            throw KeyError("signing key: ${e.message}", e)
        } catch (e: java.security.ProviderException) {
            throw KeyError("signing key: ${e.message}", e)
        }
        return signingPublicKey() ?: throw KeyError("the signing key was not stored")
    }

    private fun generate(strongBox: Boolean, biometric: Boolean) {
        val spec = KeyGenParameterSpec.Builder(SIGN_ALIAS, KeyProperties.PURPOSE_SIGN)
            .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
            .setDigests(KeyProperties.DIGEST_SHA256)
        if (biometric) {
            spec.setUserAuthenticationRequired(true)
            // Every use asks (timeout 0), strong biometrics only: no passcode fallback.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                spec.setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
            } else {
                @Suppress("DEPRECATION")
                spec.setUserAuthenticationValidityDurationSeconds(-1)
            }
            spec.setInvalidatedByBiometricEnrollment(true)
        }
        if (strongBox) spec.setIsStrongBoxBacked(true)
        KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, PROVIDER).apply { initialize(spec.build()) }.generateKeyPair()
    }

    /** The public key of the signing key, or null when there is none. Never asks for biometrics. */
    fun signingPublicKey(): ByteArray? {
        val cert = keystore().getCertificate(SIGN_ALIAS) ?: return null
        return x963(cert.publicKey as ECPublicKey)
    }

    /** Where the signing key lives: "strongBox", "tee", "software" or null (no key). For the settings screen and the tests. */
    fun signingKeyLevel(): String? {
        val key = keystore().getKey(SIGN_ALIAS, null) as? PrivateKey ?: return null
        val info = java.security.KeyFactory.getInstance(key.algorithm, PROVIDER).getKeySpec(key, android.security.keystore.KeyInfo::class.java)
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            when (info.securityLevel) {
                KeyProperties.SECURITY_LEVEL_STRONGBOX -> "strongBox"
                KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT -> "tee"
                else -> "software"
            }
        } else {
            @Suppress("DEPRECATION")
            if (info.isInsideSecureHardware) "tee" else "software"
        }
    }

    /** Whether this phone can check Class 3 (strong) biometrics now: "ready", "notEnrolled", or "unsupported". */
    fun strongBiometrics(): String = when (BiometricManager.from(context).canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_STRONG)) {
        BiometricManager.BIOMETRIC_SUCCESS -> "ready"
        BiometricManager.BIOMETRIC_ERROR_NONE_ENROLLED -> "notEnrolled"
        else -> "unsupported"
    }

    /**
     * Signs [data] (ECDSA P-256 over SHA-256, X9.62 DER) after a strong biometric check. [done] is called once, on the
     * main thread. Must be called on the main thread.
     */
    fun sign(activity: FragmentActivity, data: ByteArray, reason: String, done: (SignOutcome) -> Unit) {
        val key = try {
            keystore().getKey(SIGN_ALIAS, null) as? PrivateKey
        } catch (e: GeneralSecurityException) {
            return done(SignOutcome.Failed("keystore: ${e.message}"))
        } ?: return done(SignOutcome.Invalidated) // the key itself is gone
        val signature = Signature.getInstance(SIGNATURE)
        try {
            signature.initSign(key)
        } catch (e: KeyPermanentlyInvalidatedException) {
            return done(SignOutcome.Invalidated)
        } catch (e: GeneralSecurityException) {
            return done(SignOutcome.Failed("key: ${e.message}"))
        }
        if (software) {
            signature.update(data)
            return done(SignOutcome.Signed(signature.sign()))
        }
        if (strongBiometrics() != "ready") return done(SignOutcome.Unavailable)
        if (prompting) return done(SignOutcome.Failed("another confirmation is open"))
        prompting = true
        val callback = object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                prompting = false
                val outcome = try {
                    val s = result.cryptoObject?.signature ?: throw GeneralSecurityException("no signature object")
                    s.update(data)
                    SignOutcome.Signed(s.sign())
                } catch (e: GeneralSecurityException) {
                    SignOutcome.Failed("sign: ${e.message}")
                }
                done(outcome)
            }

            override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                prompting = false
                done(outcomeOfError(errorCode, errString.toString()))
            }
            // onAuthenticationFailed (a finger that does not match): the system prompt stays and asks again.
        }
        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle(activity.getString(R.string.phone_biometric_title))
            .setSubtitle(reason)
            .setNegativeButtonText(activity.getString(R.string.phone_biometric_cancel))
            .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG)
            .setConfirmationRequired(false)
            .build()
        BiometricPrompt(activity, ContextCompat.getMainExecutor(activity), callback).authenticate(info, BiometricPrompt.CryptoObject(signature))
    }

    /**
     * The app lock: a biometric or device-credential check to open the app, nothing signed. [done] gets "ok", "cancelled",
     * "unavailable" (only when the phone has no screen lock at all: the app cannot lock itself and opens) or "failed"
     * (every other state, a busy or locked-out sensor included, so the app stays locked). Called on the main thread;
     * [done] runs on it.
     */
    fun authenticate(activity: FragmentActivity, reason: String, done: (String) -> Unit) {
        if (software) return done("ok")
        val keyguard = activity.getSystemService(Context.KEYGUARD_SERVICE) as android.app.KeyguardManager
        if (!keyguard.isDeviceSecure) return done("unavailable")
        if (prompting) return done("failed")
        prompting = true
        val callback = object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                prompting = false
                done("ok")
            }

            override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                prompting = false
                done(
                    when (errorCode) {
                        BiometricPrompt.ERROR_USER_CANCELED, BiometricPrompt.ERROR_NEGATIVE_BUTTON, BiometricPrompt.ERROR_CANCELED -> "cancelled"
                        else -> "failed"
                    },
                )
            }
        }
        val info = BiometricPrompt.PromptInfo.Builder()
            .setTitle(activity.getString(R.string.phone_biometric_title))
            .setSubtitle(reason)
            // The screen-lock PIN is allowed as the fallback (so a busy sensor never opens the app); API 29 takes no STRONG here.
            .setAllowedAuthenticators(
                (if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) BiometricManager.Authenticators.BIOMETRIC_STRONG
                else BiometricManager.Authenticators.BIOMETRIC_WEAK) or BiometricManager.Authenticators.DEVICE_CREDENTIAL,
            )
            .setConfirmationRequired(false)
            .build()
        BiometricPrompt(activity, ContextCompat.getMainExecutor(activity), callback).authenticate(info)
    }

    fun deleteSigningKey() {
        val ks = keystore()
        if (ks.containsAlias(SIGN_ALIAS)) ks.deleteEntry(SIGN_ALIAS)
    }

    /** Removes every key: the secrets, their wrapping key and the signing key. */
    fun eraseAll() {
        dir.listFiles { f -> f.name.endsWith(".secret") || f.name.endsWith(".tmp") }?.forEach { it.delete() }
        val ks = keystore()
        if (ks.containsAlias(WRAP_ALIAS)) ks.deleteEntry(WRAP_ALIAS)
        deleteSigningKey()
    }

    companion object {
        const val PROVIDER = "AndroidKeyStore"
        private const val WRAP_ALIAS = "dev.bobix.phone.keys.wrap"
        private const val SIGN_ALIAS = "dev.bobix.phone.keys.sign"
        private const val MARKER = "keys-installed"
        private const val AES = "AES/GCM/NoPadding"
        private const val IV_BYTES = 12
        private const val SIGNATURE = "SHA256withECDSA"
        private val NAME = Regex("[a-z0-9_-]{1,32}")

        fun isDebuggable(context: Context) = context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0

        /** `04 ‖ X ‖ Y`, each coordinate 32 bytes. */
        fun x963(key: ECPublicKey): ByteArray {
            fun fixed(v: java.math.BigInteger): ByteArray {
                val b = v.toByteArray().dropWhile { it == 0.toByte() }.toByteArray()
                return ByteArray(32 - b.size) + b
            }
            return byteArrayOf(4) + fixed(key.w.affineX) + fixed(key.w.affineY)
        }

        /** What a BiometricPrompt error means for the key (see [SignOutcome]). */
        fun outcomeOfError(code: Int, message: String): SignOutcome = when (code) {
            BiometricPrompt.ERROR_USER_CANCELED, BiometricPrompt.ERROR_NEGATIVE_BUTTON, BiometricPrompt.ERROR_CANCELED -> SignOutcome.Cancelled
            BiometricPrompt.ERROR_LOCKOUT, BiometricPrompt.ERROR_LOCKOUT_PERMANENT -> SignOutcome.LockedOut
            BiometricPrompt.ERROR_NO_BIOMETRICS, BiometricPrompt.ERROR_HW_NOT_PRESENT, BiometricPrompt.ERROR_HW_UNAVAILABLE,
            BiometricPrompt.ERROR_NO_DEVICE_CREDENTIAL, BiometricPrompt.ERROR_SECURITY_UPDATE_REQUIRED -> SignOutcome.Unavailable
            else -> SignOutcome.Failed("biometric error $code: $message")
        }
    }
}
