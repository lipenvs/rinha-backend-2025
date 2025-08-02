use crate::controllers::{get_summary, new_payment, purge_payments};
use axum::{
    Extension, Router,
    routing::{get, post},
};
use redis::aio::MultiplexedConnection;

pub fn create_routes(redis: MultiplexedConnection) -> Router {
    Router::new()
        .route("/payments", post(new_payment))
        .route("/purge-payments", post(purge_payments))
        .route("/payments-summary", get(get_summary))
        .layer(Extension(redis))
}
