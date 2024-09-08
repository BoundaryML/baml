use axum::{
    extract::{self},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
};
use futures::stream;
use std::{convert::Infallible, time::Duration};
use tokio_stream::StreamExt;

#[derive(serde::Deserialize)]
pub(super) struct PingQuery {
    stream: Option<bool>,
}

pub(super) async fn ping_handler(extract::Query(query): extract::Query<PingQuery>) -> Response {
    let response = format!("pong (from baml v{})", env!("CARGO_PKG_VERSION"));
    match query.stream {
        Some(true) => {
            // Create an endless stream of "pong" messages
            let stream = stream::iter(0..)
                .map(move |i| {
                    Ok::<_, Infallible>(Event::default().data(format!("{}: seq {}", response, i)))
                })
                .throttle(Duration::from_millis(500));

            Sse::new(stream).into_response()
        }
        _ => format!("{}\n", response).into_response(),
    }
}
