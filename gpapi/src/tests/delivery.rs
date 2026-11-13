use super::*;

#[test]
fn named_download_defaults_to_empty() {
    assert_eq!(NamedDownload::default().size, None);
    assert_eq!(DownloadInfo::default().main_apk_sha256, None);
}
