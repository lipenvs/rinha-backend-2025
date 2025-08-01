use axum::{
    Router,
    routing::{get, post},
};

mod controller;
use controller::{get_summary, new_payment, purge_payments};

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/payments", post(new_payment))
        .route("/purge-payments", post(purge_payments))
        .route("/payments-summary", get(get_summary));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
