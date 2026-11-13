use crate::api_error::{ApiError, ErrorKind, Stage};
use crate::download_merge::{DOWNLOAD_TARGETS, TargetOutcome, collect_targets};
use crate::google_play_client::{Channel, GooglePlayClient, UpstreamError};
use crate::session_source::{Credentials, SessionSource, upstream_error};
use futures_util::future::join_all;
use googleplay_protobuf::DetailsResponse;
use gpapi::DownloadInfo;
use std::future::Future;
use std::sync::Arc;
use worker::{Env, console_error, console_log};

/// Everything a successful lookup reports.
pub struct Lookup {
    pub details: DetailsResponse,
    pub download: DownloadInfo,
    /// One entry per download target, in `DOWNLOAD_TARGETS` order.
    pub targets: Vec<TargetOutcome>,
}

pub struct ClientRegistry {
    sessions: SessionSource,
    env: Env,
}

impl ClientRegistry {
    #[must_use]
    pub fn new(env: Env) -> Self {
        Self {
            sessions: SessionSource::new(&env),
            env,
        }
    }

    fn env_var(&self, name: &str) -> Option<String> {
        self.env
            .var(name)
            .ok()
            .map(|value| value.to_string())
            .filter(|value| !value.is_empty())
    }

    fn channel_credentials(&self, channel: Channel) -> Result<Credentials, ApiError> {
        let prefix = channel.to_string().to_uppercase();
        let email = self.env_var(&format!("{prefix}_EMAIL"));
        let aas_token = self.env_var(&format!("{prefix}_AAS_TOKEN"));
        if let (Some(email), Some(aas_token)) = (email, aas_token) {
            Ok(Credentials { email, aas_token })
        } else {
            // Names are for the logs; clients only learn the channel.
            console_error!("{prefix}_EMAIL or {prefix}_AAS_TOKEN is not set");
            Err(ApiError::credentials_missing(channel))
        }
    }

    /// Run `op` with a logged-in client for the channel/device/ABI combination.
    ///
    /// If Google rejects the session (HTTP 401/403) before the TTL lapsed, the
    /// cache entry is dropped, the session is renewed on the same device and
    /// `op` runs exactly once more. If that is rejected too, the stored device
    /// is forgotten so the next request checks in a fresh one. Each caller
    /// retries at most once, so concurrent download targets (distinct keys)
    /// cannot multiply logins. The error stays typed until this decision is
    /// made; only then is it turned into an `ApiError`.
    async fn with_session<T, Op, Fut>(
        &self,
        channel: Channel,
        device_name: &str,
        supported_abis: &[&str],
        stage: Stage,
        op: Op,
    ) -> Result<T, ApiError>
    where
        Op: Fn(GooglePlayClient) -> Fut,
        Fut: Future<Output = Result<T, UpstreamError>>,
    {
        let abis = supported_abis.iter().copied().map(String::from).collect();
        let key = (channel, device_name.to_string(), abis);
        let credentials = self.channel_credentials(channel)?;

        let session = self.sessions.get(&key, &credentials, None).await?;
        let rejected_session_ms = session.logged_in_at_ms;
        let error = match Box::pin(op(session.client)).await {
            Ok(value) => return Ok(value),
            Err(error) if gpapi::error::is_auth_failure(&*error) => error,
            Err(error) => return Err(upstream_error(stage, channel, &error)),
        };

        console_log!("Session rejected on the {channel} channel ({error}); renewing it");
        self.sessions
            .discard(&key, rejected_session_ms, false)
            .await?;
        // The entry is gone (or was already replaced by a concurrent request's
        // fresh login), so this either renews the session or reuses that newer one.
        let session = self
            .sessions
            .get(&key, &credentials, Some(rejected_session_ms))
            .await?;
        let error = match Box::pin(op(session.client)).await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        if gpapi::error::is_auth_failure(&*error) {
            self.sessions
                .discard(&key, session.logged_in_at_ms, true)
                .await?;
        }
        Err(upstream_error(stage, channel, &error))
    }

    /// Get latest app details and merged download info across download targets.
    ///
    /// All download targets use the version advertised in the returned details.
    /// Beta and alpha lookups stop early when the app has no testing program
    /// the channel's account is enrolled in.
    ///
    /// # Errors
    ///
    /// Returns an error if details cannot be fetched, the app is unknown or
    /// unavailable on the channel, the latest version cannot be determined, or
    /// all per-ABI downloads fail.
    pub async fn get_download_info(
        &self,
        package_name: &str,
        channel: Channel,
    ) -> Result<Lookup, ApiError> {
        let Some(device_name) = self.env_var("DEVICE_NAME") else {
            console_error!("DEVICE_NAME is not set");
            return Err(ApiError::misconfigured());
        };
        let details = self
            .with_session(
                channel,
                &device_name,
                &[],
                Stage::Details,
                |client| async move { client.get_details(package_name).await },
            )
            .await?
            .ok_or_else(|| ApiError::not_found(package_name))?;
        if !channel.is_available_in(&details) {
            return Err(ApiError::channel_unavailable(channel, package_name));
        }
        let version_code = details
            .item
            .as_ref()
            .and_then(|item| item.details.as_ref())
            .and_then(|details| details.app_details.as_ref())
            .and_then(|app_details| app_details.version_code)
            .ok_or_else(|| {
                console_error!("No version code in the details of '{package_name}'");
                ApiError::new(
                    ErrorKind::Upstream,
                    format!("Google Play did not report a version for '{package_name}'"),
                )
            })?;

        // Targets are fetched concurrently; `join_all` keeps their order, which
        // decides which target's main APK and dex metadata win the merge.
        // Boxed: each download future is ~90kB, too large to hold inline.
        let results = join_all(DOWNLOAD_TARGETS.map(|target| {
            Box::pin(async move {
                self.with_session(
                    channel,
                    target.device,
                    &[target.abi],
                    Stage::Download,
                    |client| async move {
                        client
                            .get_download_info(package_name, Some(version_code))
                            .await
                    },
                )
                .await
            })
        }))
        .await;

        let (download, targets) = collect_targets(&DOWNLOAD_TARGETS, results)?;
        Ok(Lookup {
            details,
            download,
            targets,
        })
    }
}

pub type SharedClientRegistry = Arc<ClientRegistry>;

#[must_use]
pub fn shared_registry(env: &Env) -> SharedClientRegistry {
    Arc::new(ClientRegistry::new(env.clone()))
}
