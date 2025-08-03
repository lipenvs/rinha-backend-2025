use axum::{Extension, Json, extract::Query, http::StatusCode};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use redis::aio::MultiplexedConnection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize, Serialize, Debug)]
pub struct Payment {
    #[serde(rename = "correlationId")]
    correlation_id: Uuid,
    amount: f64,
    #[serde(default = "Utc::now")]
    timestamp: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct SummaryQuery {
    from: String,
    to: String,
}

#[derive(Serialize)]
pub struct SummaryResponse {
    #[serde(rename = "totalRequests")]
    total_requests: u32,
    #[serde(rename = "totalAmount")]
    total_amount: f64,
    #[serde(rename = "totalFee")]
    total_fee: f64,
    #[serde(rename = "feePerTransaction")]
    fee_per_transaction: f64,
}

pub async fn new_payment(
    Extension(mut redis): Extension<MultiplexedConnection>,
    Json(payload): Json<Payment>,
) -> StatusCode {
    match serde_json::to_string(&payload) {
        Ok(payment_json) => {
            let key = format!("payment:{}", payload.correlation_id);

            match redis.set::<String, String, ()>(key, payment_json).await {
                Ok(_) => {
                    println!("Payment salvo no Redis com sucesso");
                    StatusCode::CREATED
                }
                Err(e) => {
                    eprintln!("Erro ao salvar payment no Redis: {}", e);
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            }
        }
        Err(e) => {
            eprintln!("Erro ao serializar payment: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

pub async fn purge_payments() -> StatusCode {
    println!("POST /purge-payments - Limpeza de pagamentos executada");
    StatusCode::OK
}

pub async fn get_summary(
    Extension(mut redis): Extension<MultiplexedConnection>,
    Query(params): Query<SummaryQuery>,
) -> Result<Json<SummaryResponse>, StatusCode> {
    let from_date = match DateTime::parse_from_rfc3339(&params.from) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => {
            eprintln!("Erro ao parsear data 'from': {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let to_date = match DateTime::parse_from_rfc3339(&params.to) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => {
            eprintln!("Erro ao parsear data 'to': {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    match redis.keys::<&str, Vec<String>>("payment:*").await {
        Ok(keys) => {
            let mut total_requests = 0u32;
            let mut total_amount = 0.0f64;

            for key in keys {
                match redis.get::<String, String>(key).await {
                    Ok(payment_json) => match serde_json::from_str::<Payment>(&payment_json) {
                        Ok(payment) => {
                            if payment.timestamp >= from_date && payment.timestamp <= to_date {
                                total_requests += 1;
                                total_amount += payment.amount;
                            }
                        }
                        Err(e) => {
                            eprintln!("Erro ao deserializar payment: {}", e);
                        }
                    },
                    Err(e) => {
                        eprintln!("Erro ao buscar payment no Redis: {}", e);
                    }
                }
            }

            let fee_per_transaction = 0.05f64;
            let total_fee = total_requests as f64 * fee_per_transaction;

            let response = SummaryResponse {
                total_requests,
                total_amount,
                total_fee,
                fee_per_transaction,
            };

            Ok(Json(response))
        }
        Err(e) => {
            eprintln!("Erro ao buscar chaves no Redis: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
