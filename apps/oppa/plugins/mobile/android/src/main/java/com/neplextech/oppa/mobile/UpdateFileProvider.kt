package com.neplextech.oppa.mobile

import androidx.core.content.FileProvider

/**
 * Distinct provider class so its manifest entry can coexist with the app's
 * own `FileProvider`; it only shares verified update APKs from the cache.
 */
class UpdateFileProvider : FileProvider()
