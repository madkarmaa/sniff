// Worker entrypoint expansion creates non-Send futures for a single-threaded WASM runtime.
#![allow(clippy::future_not_send)]

mod api_error;
mod client_registry;
mod edge_cache;
mod google_play_client;
mod handlers;
mod openapi_schema;
mod package_name;
mod responses;
mod serializable_types;

use api_error::ApiError;
use client_registry::shared_registry;
use handlers::AppState;
use openapi_schema::ApiDoc;
use std::rc::Rc;
use utoipa::OpenApi;
use worker::{
    Context, Env, Method, Request, Response, Result, RouteContext, Router, console_error, event,
};

/// Path prefix of this API version, defined once for the router and the docs page.
///
/// It must equal the literal in the `utoipa::path` attribute (see
/// `handlers.rs`), the routes in `wrangler.toml` and the major version in
/// `Cargo.toml`; unit tests check all three.
pub const API_PREFIX: &str = "/v3";

fn scalar_html() -> String {
    format!(
        r#"<!doctype html>
<html>
  <head>
    <title>Sniff API Reference</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
  </head>
  <body>
    <script id="api-reference" data-url="{API_PREFIX}/openapi.json"></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference@1.69.0"></script>
  </body>
</html>"#
    )
}

// Route params are guaranteed by the router; a missing one is a 500, not a 400.
fn route_param(ctx: &RouteContext<AppState>, name: &str) -> Result<String> {
    ctx.param(name)
        .cloned()
        .ok_or_else(|| worker::Error::RustError(format!("missing {name}")))
}

// The router only accepts `fn(Request, RouteContext<_>)` handlers.
#[allow(clippy::needless_pass_by_value)]
fn redirect_to_docs(req: Request, _ctx: RouteContext<AppState>) -> Result<Response> {
    let mut url = req.url()?;
    url.set_path(&format!("{API_PREFIX}/docs"));
    Response::redirect(url)
}

#[allow(clippy::needless_pass_by_value)]
fn docs(_req: Request, _ctx: RouteContext<AppState>) -> Result<Response> {
    responses::text_ok(
        &scalar_html(),
        "text/html;charset=UTF-8",
        responses::CACHE_CONTROL_DOCS,
    )
}

#[allow(clippy::needless_pass_by_value)]
fn openapi_json(_req: Request, _ctx: RouteContext<AppState>) -> Result<Response> {
    let spec = ApiDoc::openapi()
        .to_pretty_json()
        .map_err(|e| worker::Error::RustError(e.to_string()))?;
    responses::text_ok(&spec, "application/json", responses::CACHE_CONTROL_DOCS)
}

// See `fetch` for why this future is allowed to be large and non-`Send`.
#[allow(clippy::future_not_send, clippy::large_futures)]
async fn download(req: Request, ctx: RouteContext<AppState>) -> Result<Response> {
    let package_name = route_param(&ctx, "package_name")?;
    let channel = route_param(&ctx, "channel")?;
    let request_url = req.url()?;
    handlers::get_download_info(package_name, channel, request_url, ctx.data.clone()).await
}

/// Whether `req` is a CORS preflight for an API route.
fn is_api_preflight(req: &Request) -> bool {
    req.method() == Method::Options && req.path().starts_with(&format!("{API_PREFIX}/"))
}

// See `fetch` for why this future is allowed to be large and non-`Send`.
#[allow(clippy::future_not_send, clippy::large_futures)]
async fn route(req: Request, env: Env, ctx: Context) -> Result<Response> {
    let state = AppState {
        registry: shared_registry(&env),
        ctx: Rc::new(ctx),
    };
    let docs_path = format!("{API_PREFIX}/docs");
    let spec_path = format!("{API_PREFIX}/openapi.json");
    let download_path = format!("{API_PREFIX}/download/:package_name/:channel");

    // HEAD is routed like GET; the runtime drops the body.
    Router::with_data(state)
        .get("/", redirect_to_docs)
        .head("/", redirect_to_docs)
        .get(API_PREFIX, redirect_to_docs)
        .head(API_PREFIX, redirect_to_docs)
        .get(&format!("{API_PREFIX}/"), redirect_to_docs)
        .head(&format!("{API_PREFIX}/"), redirect_to_docs)
        .get(&docs_path, docs)
        .head(&docs_path, docs)
        .get(&spec_path, openapi_json)
        .head(&spec_path, openapi_json)
        .get_async(&download_path, download)
        .head_async(&download_path, download)
        .run(req, env)
        .await
}

// Workers run on a single-threaded WASM runtime where `Send` futures are not
// required. The fetch future is large because it holds the router plus cloned
// `Env` and registry state across awaits.
#[allow(clippy::future_not_send, clippy::large_futures)]
#[event(fetch)]
async fn fetch(req: Request, env: Env, ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let response = if is_api_preflight(&req) {
        responses::preflight()
    } else {
        route(req, env, ctx).await
    };
    let response = match response {
        Ok(response) => response,
        Err(e) => {
            console_error!("Unhandled error: {e}");
            responses::error(&ApiError::internal())?
        }
    };
    responses::finalize(response)
}

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;
