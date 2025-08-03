use crate::controllers::{get_summary, purge_payments, save_payment};
use axum::{
    Extension, Router,
    routing::{get, post},
};
use redis::aio::MultiplexedConnection;

pub fn create_routes(redis: MultiplexedConnection) -> Router {
    Router::new()
        .route("/payments", post(save_payment))
        .route("/purge-payments", post(purge_payments))
        .route("/payments-summary", get(get_summary))
        .layer(Extension(redis))
}
