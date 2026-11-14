# Development standards

- Keep implementation details out of README.md.

- Keep the code simple and easy to read. Avoid complex structures and unnecessary abstractions.

- Use clear and descriptive names for variables, functions, and classes. Avoid abbreviations and single-letter names.

- Write comments only where they are needed. A comment explains why the code does something, not what it does.

- Prefer early guards in the code to reduce indentation.

- Run `cargo fmt --all` as the last step before considering a task done. Format other edited files with Prettier, except README.md, whose compact tables and examples are kept by hand.

- Run Entanglement against a baseline regularly and refactor based on its results. Every edited file must stay green, with a maintainability index of at least 20.

# Worker standards

- Keep the workspace lints in `Cargo.toml` passing. Do not add `allow` attributes to get around them; when one is truly needed, say why in a comment next to it.

- Lint the Worker for `wasm32-unknown-unknown` too, because some code only compiles for the Workers runtime.

- Keep the API version in step: `API_PREFIX`, the `utoipa::path` literal, the routes and name in `wrangler.toml` and the crate's major version. Unit tests check them.

- Report expected failures with the `ApiError` kind that fits them. Client messages stay short and generic; details, upstream bodies and URLs go to the logs only.

- Keep the `OpenAPI` annotations in `handlers.rs` and the README's API section in sync with the behavior, including status codes, headers and examples.

- Every lookup that misses the edge cache costs calls on the operator's Google accounts. Do not add upstream calls without a reason, and keep the edge cache, the shared lookups and the rate limit in front of them.

- Never hold an isolate-wide lock across an `await`, and never share I/O objects (futures, streams, responses) between requests. The Workers runtime forbids one request from using another's I/O; share plain data only. A request that waits for another must poll on a timer, because the runtime cancels a request that waits on nothing of its own as hung.

- Never log or show secrets: AAS tokens, session tokens and the contents of `.dev.vars`.

# Android standards

- Give every user-facing message a string resource in `strings.xml`.

- Keep `minSdk` at 26 and check new APIs against it; Android lint runs in CI.

# Testing standards

- Run the checks listed in the README (`cargo check`, `cargo fmt`, both clippy runs and `cargo test --workspace --locked`) before considering a Worker change done.

- Test Worker changes end to end with `bun run dev`, which reads `.dev.vars`. Its lookups use the real Google accounts, so keep them few.

- Build the app with `./gradlew :app:lintDebug :app:assembleDebug :app:assembleAndroidTest` inside `android-app/`. Run `:app:connectedDebugAndroidTest` when a device is attached.

# Git standards

- Use Conventional Commits.

- Keep commit messages short and precise.

- Make each commit one logical change. Do not bundle unrelated changes into one commit.
