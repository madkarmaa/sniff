// Fetch futures are single-threaded in Workers.
#![cfg_attr(target_arch = "wasm32", allow(clippy::future_not_send))]
//! Platform HTTP differences; protocol and validation remain in the client.
#[cfg(not(target_arch = "wasm32"))]
pub use reqwest::Response;

#[cfg(not(target_arch = "wasm32"))]
pub async fn send(request: reqwest::RequestBuilder) -> Result<Response, reqwest::Error> {
    request.send().await
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{Response, send};

#[cfg(target_arch = "wasm32")]
mod wasm {
    use js_sys::{Reflect, Uint8Array};
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    pub struct Response {
        inner: gloo_net::http::Response,
        headers: reqwest::header::HeaderMap,
        reader: Option<web_sys::ReadableStreamDefaultReader>,
    }

    impl Response {
        pub fn status(&self) -> reqwest::StatusCode {
            // Fetch only exposes valid HTTP statuses, or 0 for opaque responses.
            reqwest::StatusCode::from_u16(self.inner.status())
                .unwrap_or(reqwest::StatusCode::BAD_GATEWAY)
        }

        pub const fn headers(&self) -> &reqwest::header::HeaderMap {
            &self.headers
        }

        pub async fn bytes(self) -> Result<Vec<u8>, ()> {
            self.inner.binary().await.map_err(|_| ())
        }

        pub async fn chunk(&mut self) -> Result<Option<Vec<u8>>, ()> {
            if self.reader.is_none() {
                let Some(body) = self.inner.body() else {
                    return Ok(None);
                };
                self.reader = Some(body.get_reader().dyn_into().map_err(|_| ())?);
            }
            let reader = self.reader.as_ref().ok_or(())?;
            let result = JsFuture::from(reader.read()).await.map_err(|_| ())?;
            let done = Reflect::get(&result, &JsValue::from_str("done"))
                .map_err(|_| ())?
                .as_bool()
                .ok_or(())?;
            if done {
                return Ok(None);
            }
            let value = Reflect::get(&result, &JsValue::from_str("value")).map_err(|_| ())?;
            Ok(Some(
                value.dyn_into::<Uint8Array>().map_err(|_| ())?.to_vec(),
            ))
        }
    }

    impl Drop for Response {
        fn drop(&mut self) {
            if let Some(reader) = &self.reader {
                // Stop reading on errors, including the control-response size limit.
                let _ = reader.cancel();
                reader.release_lock();
            }
        }
    }

    pub async fn send(request: reqwest::RequestBuilder) -> Result<Response, ()> {
        let request = request.build().map_err(|_| ())?;
        let signal = web_sys::AbortSignal::timeout_with_u32(120_000);
        let mut builder = gloo_net::http::RequestBuilder::new(request.url().as_str())
            .method(request.method().clone())
            // Workers do not implement redirect="error". Return redirects to the
            // protocol layer, which rejects every non-200 status.
            .redirect(web_sys::RequestRedirect::Manual)
            .credentials(web_sys::RequestCredentials::Omit)
            .cache(web_sys::RequestCache::NoStore)
            .abort_signal(Some(&signal));
        for (name, value) in request.headers() {
            builder = builder.header(name.as_str(), value.to_str().map_err(|_| ())?);
        }
        let request = if let Some(body) = request.body() {
            let bytes = body.as_bytes().ok_or(())?;
            builder.body(Uint8Array::from(bytes)).map_err(|_| ())?
        } else {
            builder.build().map_err(|_| ())?
        };
        let response = request.send().await.map_err(|_| ())?;
        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in response.headers().entries() {
            headers.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| ())?,
                reqwest::header::HeaderValue::from_str(&value).map_err(|_| ())?,
            );
        }
        Ok(Response {
            inner: response,
            headers,
            reader: None,
        })
    }
}
