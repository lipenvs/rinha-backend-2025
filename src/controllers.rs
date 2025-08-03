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
pub struct SummaryData {
    #[serde(rename = "totalRequests")]
    total_requests: u32,
    #[serde(rename = "totalAmount")]
    total_amount: f64,
}

#[derive(Serialize)]
pub struct SummaryResponse {
    default: SummaryData,
    fallback: SummaryData,
}

pub async fn save_payment(
    Extension(mut redis): Extension<MultiplexedConnection>,
    Json(payload): Json<Payment>,
) -> StatusCode {
    match serde_json::to_string(&payload) {
        Ok(payment_json) => {
            let key = format!("queue:{}", payload.correlation_id);

            match redis.set::<String, String, ()>(key, payment_json).await {
                Ok(_) => {
                    println!("Payment {} adicionado a fila", payload.correlation_id);
                    StatusCode::CREATED
                }
                Err(e) => {
                    eprintln!("Erro ao adicionar payment na fila: {}", e);
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

pub async fn new_payments(mut redis: MultiplexedConnection) {
    match redis.keys::<&str, Vec<String>>("queue:*").await {
        Ok(keys) if keys.is_empty() => {
            return;
        }
        Ok(keys) => {
            for key in keys {
                match redis.get::<&str, String>(&key).await {
                    Ok(payment_json) => match serde_json::from_str::<Payment>(&payment_json) {
                        Ok(payment) => {
                            let external_payload = serde_json::json!({
                                "correlationId": payment.correlation_id,
                                "amount": payment.amount,
                                "requestedAt": payment.timestamp.to_rfc3339()
                            });

                            let client = reqwest::Client::new();
                            match client
                                .post("http://localhost:8001/payments")
                                .header("Content-Type", "application/json")
                                .json(&external_payload)
                                .send()
                                .await
                            {
                                Ok(response) if response.status().is_success() => {
                                    let payment_key = key.replace("queue:", "payment:");

                                    match redis
                                        .set::<String, String, ()>(payment_key, payment_json)
                                        .await
                                    {
                                        Ok(_) => match redis.del::<&str, ()>(&key).await {
                                            Ok(_) => {
                                                println!(
                                                    "Payment {} processado com sucesso",
                                                    payment.correlation_id
                                                );
                                            }
                                            Err(e) => {
                                                eprintln!(
                                                    "Erro ao remover da queue {}: {}",
                                                    key, e
                                                );
                                            }
                                        },
                                        Err(e) => {
                                            eprintln!("Erro ao salvar payment {}", e);
                                        }
                                    }
                                }
                                Ok(response) => {
                                    eprintln!(
                                        "Erro do serviço externo para {}: {}",
                                        payment.correlation_id,
                                        response.status()
                                    );
                                }
                                Err(e) => {
                                    eprintln!(
                                        "Erro na requisição HTTP para {}: {}",
                                        payment.correlation_id, e
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Erro ao deserializar payment da queue {}: {}", key, e);
                        }
                    },
                    Err(e) => {
                        eprintln!("Erro ao buscar payment da queue {}: {}", key, e);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Erro ao buscar keys da queue: {}", e);
        }
    }
}

pub async fn purge_payments(Extension(mut redis): Extension<MultiplexedConnection>) -> StatusCode {
    if let Err(e) = redis.flushdb::<()>().await {
        eprintln!("Erro ao limpar banco de dados Redis: {}", e);
        return StatusCode::INTERNAL_SERVER_ERROR;
    }
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

            let response = SummaryResponse {
                default: SummaryData {
                    total_requests,
                    total_amount,
                },
                fallback: SummaryData {
                    total_requests: 0,
                    total_amount: 0.0,
                },
            };

            Ok(Json(response))
        }
        Err(e) => {
            eprintln!("Erro ao buscar chaves no Redis: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
