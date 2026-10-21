use std::sync::Arc;

use btel_bcs::delivery::{BcsDelivery, DeliveryError};

pub(crate) async fn finish(delivery: impl Into<Arc<BcsDelivery>>) -> Result<(), DeliveryError> {
    let delivery = delivery.into();
    tokio::task::spawn_blocking(move || delivery.finish())
        .await
        .unwrap()
}
