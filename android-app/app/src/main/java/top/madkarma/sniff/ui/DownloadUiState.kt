package top.madkarma.sniff.ui

import top.madkarma.sniff.data.DownloadResult
import top.madkarma.sniff.data.UiText

data class DownloadUiState(
    val busy: Boolean = false,
    val message: UiText? = null,
    /** Overall progress from 0 to 1 while busy; null when it is not known, such as when merging. */
    val progress: Float? = null,
    val error: Boolean = false,
    val result: DownloadResult? = null,
)
