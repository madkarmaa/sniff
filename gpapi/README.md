# gpapi

A Google Play API client for Rust that runs on Cloudflare Workers. Sniff uses it to log
in with an account's AAS token, fetch app details, and get download URLs with their
sizes and hashes.

A login registers a device on the account. `Gpapi::session_state` returns what a caller
needs to persist, `Gpapi::restore_session` resumes it, and `Gpapi::reauthenticate`
renews an expired session on the same device without registering another one.

Forked from [EFForg/rs-google-play](https://github.com/EFForg/rs-google-play).
