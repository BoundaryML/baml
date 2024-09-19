use axum::{
    http::{HeaderValue, StatusCode},
    response::{
        IntoResponse, Response,
    },
};
use bytes::{BufMut, BytesMut};
use http::header;
use serde::Serialize;
use std::{io::Write};


pub(super) struct Json<T: Serialize>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    /// This is the same as axum::Json, except we default to pretty-printing JSON
    /// with a trailing newline.
    ///
    /// Reference implementation:
    /// https://docs.rs/axum/0.7.5/src/axum/json.rs.html#186-209
    fn into_response(self) -> Response {
        // Use a small initial capacity of 128 bytes like serde_json::to_vec
        // https://docs.rs/serde_json/1.0.82/src/serde_json/ser.rs.html#2189
        let mut buf = BytesMut::with_capacity(128).writer();
        match {
            match serde_json::to_writer_pretty(&mut buf, &self.0) {
                Ok(()) => write!(buf, "\n"),
                Err(e) => Err(e.into()),
            }
        } {
            Ok(()) => (
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(mime::APPLICATION_JSON.as_ref()),
                )],
                buf.into_inner().freeze(),
            )
                .into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(mime::TEXT_PLAIN_UTF_8.as_ref()),
                )],
                err.to_string(),
            )
                .into_response(),
        }
    }
}
