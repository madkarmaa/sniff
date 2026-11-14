//! Answers an edge-cache miss with a lookup that concurrent requests for the
//! same app share (see `in_flight`), and caches what may be cached.

use crate::api_error::ApiError;
use crate::edge_cache;
use crate::google_play_client::Channel;
use crate::handlers::{AppState, Caller};
use crate::in_flight::{Follower, InFlight, Progress, Turn};
use crate::openapi_schema::{ApiResponse, DownloadInfo, DownloadResponse, TargetReport};
use crate::rate_limit;
use crate::responses;
use crate::serializable_types::SerializableDetailsResponse as JsonDetails;
use std::rc::Rc;
use std::time::Duration;
use worker::{Context, Date, Delay, Response, Result, console_error};

/// A lookup's outcome as concurrent requests share it: the serialized body of
/// a success, or the error.
type SharedOutcome = std::result::Result<Rc<str>, ApiError>;

thread_local! {
    // Workers run each isolate on one thread, so this covers the isolate.
    static LOOKUPS: InFlight<SharedOutcome> = InFlight::default();
}

/// How often a follower checks on the lookup it waits for.
const FOLLOWER_CHECK_INTERVAL: Duration = Duration::from_millis(100);

/// Look `package_name` up on `channel`, or wait for the lookup another
/// request already started under `cache_key`.
///
/// # Errors
///
/// Returns a worker error if the response cannot be built.
pub async fn respond(
    cache_key: String,
    package_name: &str,
    channel: Channel,
    caller: &Caller,
    app: &AppState,
) -> Result<Response> {
    let lookups = LOOKUPS.with(InFlight::clone);
    loop {
        let leader = match lookups.join(&cache_key, Date::now().as_millis()) {
            Turn::Lead(leader) => leader,
            Turn::Follow(follower) => match wait_for(&follower).await {
                Some(outcome) => return response_for(&outcome, None),
                // The leader gave up; one of the waiting requests leads now.
                None => continue,
            },
        };
        // Only leaders reach Google, so only they count against the client.
        // Returning drops the leader, which lets its followers retry.
        if !rate_limit::allow(app.limiter.as_deref(), caller.address.as_deref()).await {
            return responses::error(&ApiError::too_many_requests());
        }
        let outcome = look_up(app, package_name, channel).await;
        leader.finish(outcome.clone());
        return response_for(&outcome, Some((&app.ctx, cache_key)));
    }
}

/// The leader's outcome, or `None` when the leader gave up. The timer keeps
/// this request busy with I/O of its own, so the runtime does not cancel it.
async fn wait_for(follower: &Follower<SharedOutcome>) -> Option<SharedOutcome> {
    loop {
        match follower.progress(Date::now().as_millis()) {
            Progress::Running => Delay::from(FOLLOWER_CHECK_INTERVAL).await,
            Progress::Finished(outcome) => return Some(outcome),
            Progress::Abandoned => return None,
        }
    }
}

async fn look_up(app: &AppState, package_name: &str, channel: Channel) -> SharedOutcome {
    let lookup = app
        .registry
        .get_download_info(package_name, channel)
        .await?;
    let body = ApiResponse {
        success: true,
        data: Some(DownloadResponse {
            details: JsonDetails(lookup.details),
            download: DownloadInfo::from(lookup.download),
            targets: lookup.targets.iter().map(TargetReport::from).collect(),
        }),
        error: None,
    };
    serde_json::to_string(&body).map(Rc::from).map_err(|e| {
        console_error!("Lookup response could not be serialized: {e}");
        ApiError::internal()
    })
}

/// The response for `outcome`. Successes and "app not found" are cacheable;
/// the leader passes `store` to put them in the edge cache. A lookup that
/// failed on every target is an error, so at worst a partial merge is cached.
fn response_for(outcome: &SharedOutcome, store: Option<(&Context, String)>) -> Result<Response> {
    let mut response = match outcome {
        Ok(body) => {
            responses::text_ok(body, "application/json", responses::CACHE_CONTROL_NO_STORE)?
        }
        Err(error) => responses::error(error)?,
    };
    let Some(ttl_secs) = edge_cache::ttl_secs(response.status_code()) else {
        return Ok(response);
    };
    response
        .headers()
        .set("Cache-Control", &edge_cache::cache_control(ttl_secs))?;
    if let Some((ctx, cache_key)) = store {
        match response.cloned() {
            Ok(copy) => edge_cache::store(ctx, cache_key, copy),
            Err(e) => console_error!("Response could not be cloned for the edge cache: {e}"),
        }
    }
    response.headers().set("X-Sniff-Cache", "MISS")?;
    Ok(response)
}
