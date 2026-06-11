package com.antflow.app

class NativeBridge {
    companion object {
        @JvmStatic
        external fun updateDownloadProgress(percent: Int, status: String, bytesFetched: Long, bytesTotal: Long)
    }
}
