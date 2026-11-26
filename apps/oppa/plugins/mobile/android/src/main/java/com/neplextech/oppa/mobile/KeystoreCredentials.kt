package com.neplextech.oppa.mobile

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Encrypts OPPA credentials with a non-exportable AES-256-GCM key held by the
 * Android Keystore. Only ciphertext is persisted, in app-private preferences
 * that are excluded from backup and device transfer.
 *
 * The account name is bound as GCM associated data, so a ciphertext copied to
 * a different account entry fails authentication instead of decrypting.
 */
internal class KeystoreCredentials(context: Context) {
    private val preferences = context.applicationContext
        .getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    @Synchronized
    fun get(account: String): String? {
        val stored = preferences.getString(entryName(account), null) ?: return null
        val packed = Base64.decode(stored, Base64.NO_WRAP)
        if (packed.size <= IV_BYTES) {
            throw IllegalStateException("stored credential is malformed")
        }
        val iv = packed.copyOfRange(0, IV_BYTES)
        val ciphertext = packed.copyOfRange(IV_BYTES, packed.size)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(TAG_BITS, iv))
        cipher.updateAAD(account.toByteArray(Charsets.UTF_8))
        val plaintext = cipher.doFinal(ciphertext)
        try {
            return String(plaintext, Charsets.UTF_8)
        } finally {
            plaintext.fill(0)
        }
    }

    @Synchronized
    fun set(account: String, value: String) {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        cipher.updateAAD(account.toByteArray(Charsets.UTF_8))
        val plaintext = value.toByteArray(Charsets.UTF_8)
        val ciphertext = try {
            cipher.doFinal(plaintext)
        } finally {
            plaintext.fill(0)
        }
        val packed = cipher.iv + ciphertext
        val committed = preferences.edit()
            .putString(entryName(account), Base64.encodeToString(packed, Base64.NO_WRAP))
            .commit()
        if (!committed) {
            throw IllegalStateException("credential could not be persisted")
        }
    }

    @Synchronized
    fun delete(account: String) {
        val committed = preferences.edit().remove(entryName(account)).commit()
        if (!committed) {
            throw IllegalStateException("credential could not be deleted")
        }
    }

    private fun key(): SecretKey {
        val keyStore = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (keyStore.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .build()
        )
        return generator.generateKey()
    }

    private fun entryName(account: String) = "credential:$account"

    companion object {
        const val PREFERENCES = "oppa_secure_credentials"
        private const val KEYSTORE = "AndroidKeyStore"
        private const val KEY_ALIAS = "oppa-credentials-v1"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val IV_BYTES = 12
        private const val TAG_BITS = 128
    }
}
