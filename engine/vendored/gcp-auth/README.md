# GCP Auth

[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![MIT licensed][mit-badge]][mit-url]

[crates-badge]: https://img.shields.io/crates/v/gcp_auth.svg
[crates-url]: https://crates.io/crates/gcp_auth
[docs-badge]: https://docs.rs/gcp_auth/badge.svg
[docs-url]: https://docs.rs/gcp_auth
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: LICENSE

GCP auth provides authentication using service accounts Google Cloud Platform (GCP)

GCP auth is a simple, minimal authentication library for Google Cloud Platform (GCP)
providing authentication using service accounts. Once authenticated, the service
account can be used to acquire bearer tokens for use in authenticating against GCP
services.

The library supports the following methods of retrieving tokens in the listed priority order:

1. Reading custom service account credentials from the path pointed to by the
   `GOOGLE_APPLICATION_CREDENTIALS` environment variable. Alternatively, custom service
   account credentials can be read from a JSON file or string.
2. Look for credentials in `.config/gcloud/application_default_credentials.json`;
   if found, use these credentials to request refresh tokens. This file can be created
   by invoking `gcloud auth application-default login`.
3. Use the default service account by retrieving a token from the metadata server.
4. Retrieving a token from the `gcloud` CLI tool, if it is available on the `PATH`.

For more detailed information and examples, see the [docs][docs-url].

This crate does not currently support Windows.

## Simple usage

The default way to use this library is to select the appropriate token provider using `provider()`. It will
find the appropriate authentication method and use it to retrieve tokens.

```rust,no_run
let provider = gcp_auth::provider().await?;
let scopes = &["https://www.googleapis.com/auth/cloud-platform"];
let token = provider.token(scopes).await?;
```

# License

Parts of the implementation have been sourced from [yup-oauth2](https://github.com/dermesser/yup-oauth2).

Licensed under [MIT license](http://opensource.org/licenses/MIT).
