//! Pixel XL uploader CLI.
//!
//! Minimal port of `gotohp` upload (`core/upload.go`, `core/api.go`): expand
//! files-or-folders, SHA-1 each file, skip hashes already remote, otherwise
//! `GetUploadToken` → `PUT` → `CommitUpload` while spoofing a Pixel XL, with
//! up to `--threads` concurrent workers (like `UploadManager`'s pool).
//! `check` reports remote presence; `download` fetches a Photos `baseUrl`
//! with the same identity.

use uploader::{client, cred};

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Args, Parser, Subcommand};

use base64::Engine as _;
use client::PhotosClient;
use cred::{Credential, parse_credential};
use sha1::Digest as _;

const DEFAULT_THREADS: usize = 3;

/// Pixel XL uploader: files-or-folders upload as a Pixel XL.
#[derive(Parser)]
#[command(
    name = "uploader",
    version,
    about = "Upload files to Google Photos while spoofing a Pixel XL"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate AAS exchange without uploading.
    CheckAuth {
        #[command(flatten)]
        auth: AuthArgs,
    },
    /// Upload files and/or folders.
    Upload {
        /// Files or folders to upload.
        #[arg(value_name = "PATH", num_args = 1.., required = true)]
        paths: Vec<PathBuf>,
        #[command(flatten)]
        auth: AuthArgs,
        #[command(flatten)]
        scan: ScanArgs,
        /// Upload even when the hash is already remote.
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// Check whether file hashes are already remote.
    Check {
        /// Files or folders to check.
        #[arg(value_name = "PATH", num_args = 1.., required = true)]
        paths: Vec<PathBuf>,
        #[command(flatten)]
        auth: AuthArgs,
        #[command(flatten)]
        scan: ScanArgs,
    },
    /// Download an original by media key or exact original URL.
    Download {
        /// Photos baseUrl to fetch (`=d` appended when bare).
        #[arg(
            long,
            value_name = "ORIGINAL_URL",
            conflicts_with = "media_key",
            required_unless_present = "media_key"
        )]
        url: Option<String>,
        /// Native media key; resolves and verifies original bytes.
        #[arg(long, conflicts_with = "url")]
        media_key: Option<String>,
        /// Output path (never overwritten).
        #[arg(long, value_name = "PATH")]
        out: PathBuf,
        #[command(flatten)]
        auth: AuthArgs,
    },
}

/// Auth flags shared by every subcommand.
#[derive(Args)]
struct AuthArgs {
    /// Raw gotohp credential query string (or $`GOTOHP_CREDENTIAL`).
    #[arg(
        long,
        env = "GOTOHP_CREDENTIAL",
        hide_env_values = true,
        value_name = "QUERY"
    )]
    credential: Option<String>,
    /// File holding the raw credential query string.
    #[arg(long = "credential-file", value_name = "PATH")]
    credential_file: Option<PathBuf>,
    /// Runtime-only dotenv input for `STABLE_EMAIL` and `STABLE_AAS_TOKEN`.
    #[arg(long, default_value = ".env")]
    env_file: PathBuf,
}

/// Folder-scan flags shared by `upload` and `check`.
#[derive(Args)]
struct ScanArgs {
    /// Scan folders recursively.
    #[arg(long, short = 'r')]
    recursive: bool,
    /// Max concurrent workers.
    #[arg(
        long,
        short = 't',
        default_value_t = DEFAULT_THREADS,
        value_parser = parse_thread_count
    )]
    threads: usize,
    /// Skip subdirectories by exact name (e.g. @eaDir).
    #[arg(long, short = 'e', value_name = "NAME")]
    exclude: Option<String>,
}

/// Parse `--threads`/`-t` into a positive worker count.
///
/// # Errors
///
/// Returns an error when the value is not a number or is zero.
fn parse_thread_count(raw: &str) -> Result<usize, String> {
    let threads: usize = raw
        .trim()
        .parse()
        .map_err(|_| format!("invalid --threads {raw:?}, want a positive integer"))?;
    if threads == 0 {
        return Err(format!("threads must be a positive integer, got {threads}"));
    }
    Ok(threads)
}

/// Resolve the credential from `--credential`, `--credential-file`, or `$GOTOHP_CREDENTIAL`.
///
/// Clap fills `--credential` from the environment; the file is read only when
/// the flag (and env) is absent.
///
/// # Errors
///
/// Returns an error when no credential is found or it fails to parse.
fn resolve_credential(
    credential: Option<String>,
    credential_file: Option<PathBuf>,
    env_file: &Path,
) -> Result<Credential, String> {
    if let Some(query) = credential {
        return parse_credential(&query);
    }
    if let Some(path) = credential_file {
        let raw = fs::read_to_string(&path).map_err(|e| format!("read credential file: {e}"))?;
        return parse_credential(&raw);
    }
    let mut email = std::env::var("STABLE_EMAIL").ok();
    let mut token = std::env::var("STABLE_AAS_TOKEN").ok();
    if email.is_none() || token.is_none() {
        for entry in dotenvy::from_path_iter(env_file)
            .map_err(|_| "cannot open dotenv credentials".to_string())?
        {
            let (key, value) = entry.map_err(|_| "invalid dotenv credentials".to_string())?;
            match key.as_str() {
                "STABLE_EMAIL" if email.is_none() => email = Some(value),
                "STABLE_AAS_TOKEN" if token.is_none() => token = Some(value),
                _ => {}
            }
        }
    }
    cred::from_aas(
        &email.ok_or("missing STABLE_EMAIL")?,
        &token.ok_or("missing STABLE_AAS_TOKEN")?,
    )
}

/// Expand input paths into a deduplicated file list.
///
/// Files pass through; folders contribute their top-level files, or the full
/// tree with `recursive` (mirroring `gotohp`'s directory scan). `exclude`
/// skips subdirectories by exact name, like `gotohp --exclude`.
///
/// # Errors
///
/// Returns an error when a path is inaccessible or a folder is unreadable.
fn expand_paths(
    inputs: &[PathBuf],
    recursive: bool,
    exclude: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for input in inputs {
        let metadata =
            fs::metadata(input).map_err(|e| format!("access {}: {e}", input.display()))?;
        if metadata.is_dir() {
            collect_dir(input, recursive, exclude, &mut out, &mut seen)?;
        } else if metadata.is_file() {
            push_unique(input, &mut out, &mut seen);
        } else {
            return Err(format!("not a file or directory: {}", input.display()));
        }
    }
    Ok(out)
}

/// Push `path` unless its canonical form was already collected.
fn push_unique(path: &Path, out: &mut Vec<PathBuf>, seen: &mut HashSet<String>) {
    if seen.insert(canonical_key(path)) {
        out.push(path.to_path_buf());
    }
}

/// Canonical dedup key, falling back to the literal path.
fn canonical_key(path: &Path) -> String {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical.to_string_lossy().into_owned();
    }
    path.to_string_lossy().into_owned()
}

/// Append a folder's files, recursing when asked.
///
/// # Errors
///
/// Returns an error when the folder or an entry cannot be read.
fn collect_dir(
    dir: &Path,
    recursive: bool,
    exclude: Option<&str>,
    out: &mut Vec<PathBuf>,
    seen: &mut HashSet<String>,
) -> Result<(), String> {
    if !seen.insert(format!("dir:{}", canonical_key(dir))) {
        return Ok(());
    }
    let entries = fs::read_dir(dir).map_err(|e| format!("scan {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read entry in {}: {e}", dir.display()))?;
        let path = entry.path();
        let metadata = fs::metadata(&path).map_err(|e| format!("stat {}: {e}", path.display()))?;
        if metadata.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if let Some(pattern) = exclude
                && name == pattern
            {
                continue;
            }
            if recursive {
                collect_dir(&path, recursive, exclude, out, seen)?;
            }
        } else if metadata.is_file() {
            push_unique(&path, out, seen);
        }
    }
    Ok(())
}

/// Stream a file into SHA-1, returning digest and size.
///
/// # Errors
///
/// Returns an error when the file cannot be read.
fn sha1_of_file(path: &Path) -> Result<([u8; 20], u64), String> {
    let file = fs::File::open(path).map_err(|e| format!("open file: {e}"))?;
    let mut hasher = sha1::Sha1::new();
    let mut reader = std::io::BufReader::new(file);
    let mut total: u64 = 0;
    loop {
        use std::io::Read as _;
        let mut chunk = [0_u8; 8192];
        let read = reader
            .read(&mut chunk)
            .map_err(|e| format!("hash file: {e}"))?;
        if read == 0 {
            break;
        }
        let bytes = chunk
            .get(..read)
            .ok_or_else(|| "hash chunk slice".to_string())?;
        hasher.update(bytes);
        let delta = u64::try_from(read).map_err(|e| format!("chunk size: {e}"))?;
        total = total
            .checked_add(delta)
            .ok_or_else(|| "file too large".to_string())?;
    }
    let digest = hasher.finalize();
    let bytes: &[u8] = digest.as_ref();
    let sha1: [u8; 20] = bytes
        .try_into()
        .map_err(|_| "unexpected SHA-1 length".to_string())?;
    Ok((sha1, total))
}

/// File mtime as unix seconds, falling back to now.
///
/// # Errors
///
/// Returns an error only when the clock itself fails.
fn mtime_unix(path: &Path) -> Result<i64, String> {
    let metadata = fs::metadata(path).map_err(|e| format!("stat file: {e}"))?;
    if let Ok(modified) = metadata.modified()
        && let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH)
        && let Ok(secs) = i64::try_from(since.as_secs())
    {
        return Ok(secs);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("clock: {e}"))?;
    i64::try_from(now.as_secs()).map_err(|e| format!("clock overflow: {e}"))
}

/// Write without overwriting (like `converter::write_new`).
///
/// # Errors
///
/// Returns an error when the path exists or the write fails.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("create output (never overwrites): {e}"))?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(format!("write output: {error}"));
    }
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .concat()
}

fn file_name_of(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|s| s.to_str())
        .map(std::string::ToString::to_string)
        .ok_or_else(|| "file name must be valid UTF-8".to_string())
}

/// One file's upload result; the path travels with the outcome so
/// concurrently completed tasks stay attributable.
struct FileResult {
    path: PathBuf,
    outcome: UploadOutcome,
}

/// Per-file upload outcome.
enum UploadOutcome {
    Uploaded { sha1_hex: String, media_key: String },
    Skipped { sha1_hex: String, media_key: String },
    Failed { error: String },
}

/// One file's check result.
struct CheckResult {
    path: PathBuf,
    outcome: CheckOutcome,
}

/// Per-file check outcome; `media_key` is `Some` when the hash is remote.
enum CheckOutcome {
    Done {
        sha1_hex: String,
        media_key: Option<String>,
    },
    Failed {
        error: String,
    },
}

/// Read a file for PUT, rejecting concurrent modification.
///
/// # Errors
///
/// Returns an error when the file cannot be read or changed size since hashing.
fn read_for_upload(path: &Path, size: u64, expected_sha1: &[u8; 20]) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|e| format!("read file: {e}"))?;
    let len = u64::try_from(bytes.len()).map_err(|e| format!("file len: {e}"))?;
    if len != size || sha1::Sha1::digest(&bytes).as_slice() != expected_sha1 {
        return Err("file changed during upload".to_string());
    }
    Ok(bytes)
}

/// Upload one file: hash → dedup check → token → PUT → commit.
///
/// Ports `gotohp/core/upload.go::uploadSingleFile`.
///
/// # Errors
///
/// Returns an error when any upload step fails.
async fn upload_file(path: &Path, cred: Credential, force: bool) -> Result<UploadOutcome, String> {
    let (sha1, size) = sha1_of_file(path)?;
    let sha1_hex = hex_lower(&sha1);
    let file_name = file_name_of(path)?;
    let mtime = mtime_unix(path)?;
    let mut api = PhotosClient::new(cred).map_err(|e| e.to_string())?;
    if !force
        && let Some(media_key) = api
            .find_remote_media_by_hash(&sha1)
            .await
            .map_err(|e| e.to_string())?
    {
        return Ok(UploadOutcome::Skipped {
            sha1_hex,
            media_key,
        });
    }
    let sha1_b64 = base64::engine::general_purpose::STANDARD.encode(sha1);
    let upload_id = api
        .get_upload_token(&sha1_b64, size)
        .await
        .map_err(|e| e.to_string())?;
    let bytes = read_for_upload(path, size, &sha1)?;
    let scotty = api
        .put_upload(&upload_id, bytes)
        .await
        .map_err(|e| e.to_string())?;
    let media_key = api
        .commit_upload(&scotty, &file_name, &sha1, mtime)
        .await
        .map_err(|e| e.to_string())?;
    Ok(UploadOutcome::Uploaded {
        sha1_hex,
        media_key,
    })
}

/// Check one file's hash against the remote library.
///
/// # Errors
///
/// Returns an error when hashing or the lookup fails.
async fn check_file(path: &Path, cred: Credential) -> Result<CheckOutcome, String> {
    let (sha1, _) = sha1_of_file(path)?;
    let sha1_hex = hex_lower(&sha1);
    let mut api = PhotosClient::new(cred).map_err(|e| e.to_string())?;
    let media_key = api
        .find_remote_media_by_hash(&sha1)
        .await
        .map_err(|e| e.to_string())?;
    Ok(CheckOutcome::Done {
        sha1_hex,
        media_key,
    })
}

/// Upload every file with up to `threads` concurrent workers.
///
/// Mirrors `gotohp`'s pool (`core/upload.go`: `numWorkers :=
/// min(opts.Threads, len(workItems))`): each task owns its client, per-file
/// lines print in input order, and the run fails when any file failed.
///
/// # Errors
///
/// Returns an error summarizing failures when any file failed.
// The semaphore permit is the worker slot: it is intentionally held for the
// whole task, so the lint's early-drop suggestion does not apply here.
#[allow(clippy::significant_drop_tightening)]
async fn run_upload_batch(
    files: Vec<PathBuf>,
    cred: Credential,
    force: bool,
    threads: usize,
) -> Result<(), String> {
    if files.is_empty() {
        println!("NO_FILES nothing to upload");
        return Ok(());
    }
    let workers = threads.min(files.len());
    let semaphore = Arc::new(tokio::sync::Semaphore::new(workers));
    let mut handles = Vec::new();
    let mut paths = Vec::new();
    for file in files {
        paths.push(file.clone());
        let task_cred = cred.clone();
        let task_semaphore = Arc::clone(&semaphore);
        handles.push(tokio::spawn(async move {
            let permit = match task_semaphore.acquire_owned().await {
                Ok(permit) => permit,
                Err(error) => {
                    return FileResult {
                        path: file,
                        outcome: UploadOutcome::Failed {
                            error: format!("semaphore: {error}"),
                        },
                    };
                }
            };
            let _permit = permit;
            let outcome = upload_file(&file, task_cred, force)
                .await
                .unwrap_or_else(|error| UploadOutcome::Failed { error });
            FileResult {
                path: file,
                outcome,
            }
        }));
    }
    let mut ok_count: usize = 0;
    let mut skip_count: usize = 0;
    let mut fail_count: usize = 0;
    for (handle, path) in handles.into_iter().zip(paths) {
        let result = handle.await.unwrap_or_else(|error| FileResult {
            path,
            outcome: UploadOutcome::Failed {
                error: format!("worker: {error}"),
            },
        });
        match &result.outcome {
            UploadOutcome::Uploaded {
                sha1_hex,
                media_key,
            } => {
                println!(
                    "OK path={} sha1={sha1_hex} mediaKey={media_key}",
                    result.path.display()
                );
                ok_count = ok_count.saturating_add(1);
            }
            UploadOutcome::Skipped {
                sha1_hex,
                media_key,
            } => {
                println!(
                    "SKIPPED path={} sha1={sha1_hex} mediaKey={media_key}",
                    result.path.display()
                );
                skip_count = skip_count.saturating_add(1);
            }
            UploadOutcome::Failed { error } => {
                println!("ERROR path={} error={error}", result.path.display());
                fail_count = fail_count.saturating_add(1);
            }
        }
    }
    println!("done ok={ok_count} skipped={skip_count} failed={fail_count}");
    if fail_count > 0 {
        return Err(format!("{fail_count} file(s) failed"));
    }
    Ok(())
}

/// Check every file with up to `threads` concurrent lookups.
///
/// # Errors
///
/// Returns an error summarizing failures when any file failed.
// The semaphore permit is the worker slot: it is intentionally held for the
// whole task, so the lint's early-drop suggestion does not apply here.
#[allow(clippy::significant_drop_tightening)]
async fn run_check_batch(
    files: Vec<PathBuf>,
    cred: Credential,
    threads: usize,
) -> Result<(), String> {
    if files.is_empty() {
        println!("NO_FILES nothing to check");
        return Ok(());
    }
    let workers = threads.min(files.len());
    let semaphore = Arc::new(tokio::sync::Semaphore::new(workers));
    let mut handles = Vec::new();
    let mut paths = Vec::new();
    for file in files {
        paths.push(file.clone());
        let task_cred = cred.clone();
        let task_semaphore = Arc::clone(&semaphore);
        handles.push(tokio::spawn(async move {
            let permit = match task_semaphore.acquire_owned().await {
                Ok(permit) => permit,
                Err(error) => {
                    return CheckResult {
                        path: file,
                        outcome: CheckOutcome::Failed {
                            error: format!("semaphore: {error}"),
                        },
                    };
                }
            };
            let _permit = permit;
            let outcome = check_file(&file, task_cred)
                .await
                .unwrap_or_else(|error| CheckOutcome::Failed { error });
            CheckResult {
                path: file,
                outcome,
            }
        }));
    }
    let mut fail_count: usize = 0;
    for (handle, path) in handles.into_iter().zip(paths) {
        let result = handle.await.unwrap_or_else(|error| CheckResult {
            path,
            outcome: CheckOutcome::Failed {
                error: format!("worker: {error}"),
            },
        });
        match &result.outcome {
            CheckOutcome::Done {
                sha1_hex,
                media_key: Some(media_key),
            } => {
                println!(
                    "FOUND path={} sha1={sha1_hex} mediaKey={media_key}",
                    result.path.display()
                );
            }
            CheckOutcome::Done {
                sha1_hex,
                media_key: None,
            } => {
                println!("NOT_FOUND path={} sha1={sha1_hex}", result.path.display());
            }
            CheckOutcome::Failed { error } => {
                println!("ERROR path={} error={error}", result.path.display());
                fail_count = fail_count.saturating_add(1);
            }
        }
    }
    if fail_count > 0 {
        return Err(format!("{fail_count} file(s) failed"));
    }
    Ok(())
}

/// Run `download`: fetch a Photos `baseUrl` with Pixel XL identity.
///
/// # Errors
///
/// Returns an error when the download or write fails.
async fn run_download(
    url: Option<&str>,
    media_key: Option<&str>,
    out: &Path,
    cred: Credential,
) -> Result<(), String> {
    let mut api = PhotosClient::new(cred).map_err(|e| e.to_string())?;
    let bytes = if let Some(id) = media_key {
        api.download(id).await.map_err(|e| e.to_string())?
    } else {
        api.download_url(url.ok_or("missing original URL")?)
            .await
            .map_err(|e| e.to_string())?
    };
    write_new(out, &bytes)?;
    println!("OK bytes={} out={}", bytes.len(), out.display());
    Ok(())
}

/// Expand scan paths, dropping a blank `--exclude`.
///
/// # Errors
///
/// Returns an error when a path is inaccessible or a folder is unreadable.
fn expand_scan(paths: &[PathBuf], scan: &ScanArgs) -> Result<Vec<PathBuf>, String> {
    let exclude = scan
        .exclude
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    expand_paths(paths, scan.recursive, exclude)
}

async fn run_async() -> Result<(), String> {
    match Cli::parse().command {
        Command::CheckAuth { auth } => {
            let cred = resolve_credential(auth.credential, auth.credential_file, &auth.env_file)?;
            let mut api = PhotosClient::new(cred).map_err(|e| e.to_string())?;
            let _token = api.bearer_token().await.map_err(|e| e.to_string())?;
            println!("AAS exchange succeeded");
            Ok(())
        }
        Command::Upload {
            paths,
            auth,
            scan,
            force,
        } => {
            let cred = resolve_credential(auth.credential, auth.credential_file, &auth.env_file)?;
            let files = expand_scan(&paths, &scan)?;
            run_upload_batch(files, cred, force, scan.threads).await
        }
        Command::Check { paths, auth, scan } => {
            let cred = resolve_credential(auth.credential, auth.credential_file, &auth.env_file)?;
            let files = expand_scan(&paths, &scan)?;
            run_check_batch(files, cred, scan.threads).await
        }
        Command::Download {
            url,
            media_key,
            out,
            auth,
        } => {
            let cred = resolve_credential(auth.credential, auth.credential_file, &auth.env_file)?;
            run_download(url.as_deref(), media_key.as_deref(), &out, cred).await
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(error) = run_async().await {
        eprintln!("{error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn try_parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(args.iter().copied())
    }

    #[test]
    fn changed_same_size_file_is_not_uploaded() {
        let path = std::env::temp_dir().join(format!("sniff-mutated-{}", std::process::id()));
        fs::write(&path, b"old").unwrap();
        let (hash, size) = sha1_of_file(&path).unwrap();
        fs::write(&path, b"new").unwrap();
        assert!(read_for_upload(&path, size, &hash).is_err());
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn download_requires_one_selector() {
        assert!(try_parse(&["uploader", "download", "--out", "a"]).is_err());
        assert!(try_parse(&["uploader", "download", "--out", "a", "--media-key", "id"]).is_ok());
        assert!(
            try_parse(&[
                "uploader",
                "download",
                "--out",
                "a",
                "--media-key",
                "id",
                "--url",
                "url"
            ])
            .is_err()
        );
    }

    #[test]
    fn upload_defaults_and_multi_paths() {
        let cli =
            try_parse(&["uploader", "upload", "a.jpg", "b.jpg", "--credential", "Q"]).unwrap();
        let Command::Upload {
            paths, scan, force, ..
        } = cli.command
        else {
            panic!("wrong subcommand")
        };
        assert_eq!(paths, vec![PathBuf::from("a.jpg"), PathBuf::from("b.jpg")]);
        assert_eq!(scan.threads, DEFAULT_THREADS);
        assert!(!force);
        assert!(!scan.recursive);
    }

    #[test]
    fn upload_threads_short_and_zero_rejected() {
        let cli = try_parse(&["uploader", "upload", "-t", "7", "a.jpg"]).unwrap();
        let Command::Upload { scan, .. } = cli.command else {
            panic!("wrong subcommand")
        };
        assert_eq!(scan.threads, 7);
        assert!(try_parse(&["uploader", "upload", "--threads=0", "a.jpg"]).is_err());
    }

    #[test]
    fn unknown_flags_rejected() {
        assert!(try_parse(&["uploader", "upload", "a.jpg", "--bogus"]).is_err());
    }

    #[test]
    fn expand_files_dirs_and_exclude() {
        let base = std::env::temp_dir().join(format!("uploader-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("sub")).unwrap();
        fs::create_dir_all(base.join("@eaDir")).unwrap();
        fs::write(base.join("a.bin"), b"a").unwrap();
        fs::write(base.join("sub").join("b.bin"), b"b").unwrap();
        fs::write(base.join("@eaDir").join("c.bin"), b"c").unwrap();

        let one = expand_paths(&[base.join("a.bin")], false, None).unwrap();
        assert_eq!(one.len(), 1);

        let top = expand_paths(std::slice::from_ref(&base), false, None).unwrap();
        assert_eq!(top.len(), 1);

        let all = expand_paths(std::slice::from_ref(&base), true, None).unwrap();
        assert_eq!(all.len(), 3);

        let filtered = expand_paths(std::slice::from_ref(&base), true, Some("@eaDir")).unwrap();
        assert_eq!(filtered.len(), 2);

        let dup = expand_paths(&[base.join("a.bin"), base.join("a.bin")], false, None).unwrap();
        assert_eq!(dup.len(), 1);

        let _ = fs::remove_dir_all(&base);
    }
}
