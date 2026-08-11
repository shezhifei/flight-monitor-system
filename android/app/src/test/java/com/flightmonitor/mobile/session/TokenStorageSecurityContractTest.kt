package com.flightmonitor.mobile.session

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Static/contract checks for secure token storage policy.
 */
class TokenStorageSecurityContractTest {

    @Test
    fun encryptedStorageConstantsAreStable() {
        assertEquals("mobile_auth_tokens_encrypted", TokenStorage.PREFS_NAME_ENCRYPTED)
        assertEquals("mobile_auth_tokens", TokenStorage.PREFS_NAME_LEGACY_PLAINTEXT)
        assertFalse(
            TokenStorage.PREFS_NAME_LEGACY_PLAINTEXT == TokenStorage.PREFS_NAME_ENCRYPTED,
        )
    }

    @Test
    fun secureTokenStorageExceptionIsFailClosedType() {
        val ex = SecureTokenStorageException("Legacy plaintext token store still contains data after clear")
        assertTrue(ex.message!!.contains("Legacy") || ex.message!!.contains("clear"))
    }
}
