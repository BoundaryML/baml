//! reqwest-backed implementation used on wasm32 (browser fetch): pure
//! re-exports, behaviorally identical to using reqwest directly.

pub use reqwest::{
    get, header, Body, Client, ClientBuilder, Error, IntoUrl, Method, Request, RequestBuilder,
    Response, Result, StatusCode, Url,
};
