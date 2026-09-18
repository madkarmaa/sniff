mod client_registry;
mod google_play_client;
mod handlers;
mod openapi_schema;
mod serializable_types;

use client_registry::{SharedClientRegistry, shared_registry};
use openapi_schema::ApiDoc;
use utoipa::OpenApi;
use worker::{Context, Env, Headers, Request, Response, Result, RouteContext, Router, event};

const SCALAR_HTML: &str = r#"<!doctype html>
<html>
  <head>
    <title>Sniff API Reference</title>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
  </head>
  <body>
    <script id="api-reference" data-url="/openapi.json"></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference@1.69.0"></script>
  </body>
</html>"#;

// Route params are guaranteed by the router; a missing one is a 500, not a 400.
fn route_param(ctx: &RouteContext<SharedClientRegistry>, name: &str) -> Result<String> {
    ctx.param(name)
        .cloned()
        .ok_or_else(|| worker::Error::RustError(format!("missing {name}")))
}

// Workers run on a single-threaded WASM runtime where `Send` futures are not
// required. The fetch future is large because it holds the router plus cloned
// `Env` and registry state across awaits.
#[allow(clippy::future_not_send, clippy::large_futures)]
#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let router = Router::with_data(shared_registry(&env));

    router
        .get("/", |req, _ctx| {
            let mut url = req.url()?;
            url.set_path("/docs");
            Response::redirect(url)
        })
        .get("/docs", |_req, _ctx| {
            let headers = Headers::new();
            headers.set("Content-Type", "text/html;charset=UTF-8")?;

            Ok(Response::ok(SCALAR_HTML)?.with_headers(headers))
        })
        .get("/openapi.json", |_req, _ctx| {
            let spec = ApiDoc::openapi()
                .to_pretty_json()
                .map_err(|e| worker::Error::RustError(e.to_string()))?;

            let headers = Headers::new();
            headers.set("Content-Type", "application/json")?;

            Ok(Response::ok(&spec)?.with_headers(headers))
        })
        .get_async("/v1/details/:package_name", |_req, ctx| async move {
            let package_name = route_param(&ctx, "package_name")?;
            handlers::get_details_multi(package_name, ctx.data.clone()).await
        })
        .get_async(
            "/v1/details/:package_name/:channel",
            |_req, ctx| async move {
                let package_name = route_param(&ctx, "package_name")?;
                let channel = route_param(&ctx, "channel")?;
                handlers::get_details_single(package_name, channel, ctx.data.clone()).await
            },
        )
        .get_async(
            "/v1/download/:package_name/:channel/:version_code",
            |_req, ctx| async move {
                let package_name = route_param(&ctx, "package_name")?;
                let channel = route_param(&ctx, "channel")?;
                let version_code: i64 = ctx
                    .param("version_code")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                handlers::get_download_info(package_name, channel, version_code, ctx.data.clone())
                    .await
            },
        )
        .run(req, env)
        .await
}
