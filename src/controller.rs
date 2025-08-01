use axum::{Json, http::StatusCode};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize, Debug)]
pub struct Payment {
    correlationId: Uuid,
    amount: f64,
}

pub async fn new_payment(Json(payload): Json<Payment>) -> StatusCode {
    println!("POST /payments - Novo pagamento recebido: {:?}", payload);
    StatusCode::CREATED
}

pub async fn purge_payments() -> StatusCode {
    println!("POST /purge-payments - Limpeza de pagamentos executada");
    StatusCode::OK
}

pub async fn get_summary() -> StatusCode {
    println!("GET /payments-summary - Resumo de pagamentos solicitado");
    StatusCode::OK
}
