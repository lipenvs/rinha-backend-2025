use axum::{Extension, Json, extract::Query, http::StatusCode};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use redis::aio::MultiplexedConnection;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
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
            match redis
                .rpush::<&str, String, ()>("payments_queue", payment_json)
                .await
            {
                Ok(_) => {
                    println!("Payment {} adicionado a fila", payload.correlation_id);
                    StatusCode::ACCEPTED
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

pub async fn start_payment_workers(redis: MultiplexedConnection) {
    let client = Arc::new(reqwest::Client::new());

    let redis_clone = redis.clone();
    let client_clone = client.clone();

    tokio::spawn(async move {
        println!("Iniciando worker de processamento de pagamentos...");
        new_payments_worker(redis_clone, client_clone).await;
    });
}

pub async fn new_payments_worker(mut redis: MultiplexedConnection, client: Arc<reqwest::Client>) {
    loop {
        match redis
            .lpop::<&str, Option<String>>("payments_queue", None)
            .await
        {
            Ok(Some(payment_json)) => {
                println!("Worker: Processando payment");
                process_single_payment(payment_json, client.clone(), redis.clone()).await;
            }
            Ok(None) => {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
            Err(e) => {
                eprintln!("Worker: Erro ao buscar payment da fila: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            }
        }
    }
}

async fn process_single_payment(
    payment_json: String,
    client: Arc<reqwest::Client>,
    mut redis: MultiplexedConnection,
) {
    match serde_json::from_str::<Payment>(&payment_json) {
        Ok(payment) => {
            if payment.amount <= 0.0 {
                eprintln!(
                    "Worker: Amount inválido para {}: {}",
                    payment.correlation_id, payment.amount
                );
                return;
            }

            let external_payload = serde_json::json!({
                "correlationId": payment.correlation_id,
                "amount": payment.amount,
                "requestedAt": payment.timestamp.to_rfc3339()
            });

            let primary_url = "http://payment-processor-default:8080/payments";
            let fallback_url = "http://payment-processor-fallback:8080/payments";

            let mut success = false;
            let mut service_used = "none";

            match client
                .post(primary_url)
                .header("Content-Type", "application/json")
                .json(&external_payload)
                .timeout(std::time::Duration::from_secs(3))
                .send()
                .await
            {
                Ok(response) if response.status().is_success() => {
                    success = true;
                    service_used = "default";
                }
                Ok(response) => {
                    let status = response.status();
                    if status.as_u16() == 422 {
                        success = true;
                        service_used = "default";
                    } else {
                        println!(
                            "Worker: Serviço principal falhou para {}: {}",
                            payment.correlation_id, status
                        );
                    }
                }
                Err(e) => {
                    println!(
                        "Worker: Erro na requisição para serviço principal {}: {}",
                        payment.correlation_id, e
                    );
                }
            }

            if !success {
                match client
                    .post(fallback_url)
                    .header("Content-Type", "application/json")
                    .json(&external_payload)
                    .timeout(std::time::Duration::from_secs(3))
                    .send()
                    .await
                {
                    Ok(response) if response.status().is_success() => {
                        success = true;
                        service_used = "fallback";
                        println!(
                            "Worker: Payment {} processado no serviço fallback",
                            payment.correlation_id
                        );
                    }
                    Ok(response) => {
                        let status = response.status();
                        if status.as_u16() == 422 {
                            success = true;
                            service_used = "fallback";
                            println!(
                                "Worker: Payment {} já existia no serviço fallback",
                                payment.correlation_id
                            );
                        } else {
                            println!(
                                "Worker: Serviço fallback também falhou para {}: {}",
                                payment.correlation_id, status
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Worker: Erro na requisição para serviço fallback {}: {}",
                            payment.correlation_id, e
                        );
                    }
                }
            }

            if success {
                let payment_key = format!("payment:{}", payment.correlation_id);
                if let Err(e) = redis
                    .set::<String, String, ()>(payment_key, payment_json)
                    .await
                {
                    eprintln!(
                        "Worker: Erro ao salvar payment processado {}: {}",
                        payment.correlation_id, e
                    );
                } else {
                    let service_key = format!("summary:service:{}", service_used);
                    let _: Result<(), _> = redis
                        .hincr::<String, &str, u32, ()>(service_key.clone(), "total_requests", 1)
                        .await;

                    let amount_key = format!("{}:total_amount", service_key);
                    let current_amount: f64 = redis
                        .get::<String, Option<f64>>(amount_key.clone())
                        .await
                        .unwrap_or(Some(0.0))
                        .unwrap_or(0.0);
                    let _: Result<(), _> = redis
                        .set::<String, f64, ()>(amount_key, current_amount + payment.amount)
                        .await;
                }
            } else {
                let _: Result<(), _> = redis
                    .rpush::<&str, String, ()>("payments_queue", payment_json)
                    .await;
                eprintln!(
                    "Worker: Payment {} recolocado na fila para nova tentativa",
                    payment.correlation_id
                );
            }
        }
        Err(e) => {
            eprintln!("Worker: Erro ao deserializar payment: {}", e);
        }
    }
}

pub async fn purge_payments(Extension(mut redis): Extension<MultiplexedConnection>) -> StatusCode {
    if let Err(e) = redis.del::<&str, ()>("payments_queue").await {
        eprintln!("Erro ao limpar fila de payments: {}", e);
    }

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
    let _from_date = match DateTime::parse_from_rfc3339(&params.from) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => {
            eprintln!("Erro ao parsear data 'from': {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let _to_date = match DateTime::parse_from_rfc3339(&params.to) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => {
            eprintln!("Erro ao parsear data 'to': {}", e);
            return Err(StatusCode::BAD_REQUEST);
        }
    };

    let mut default_requests = 0u32;
    let mut default_amount = 0.0f64;
    let mut fallback_requests = 0u32;
    let mut fallback_amount = 0.0f64;

    if let Ok(requests) = redis
        .hget::<&str, &str, Option<u32>>("summary:service:default", "total_requests")
        .await
    {
        default_requests = requests.unwrap_or(0);
    }
    if let Ok(amount) = redis
        .get::<&str, Option<f64>>("summary:service:default:total_amount")
        .await
    {
        default_amount = amount.unwrap_or(0.0);
    }

    if let Ok(requests) = redis
        .hget::<&str, &str, Option<u32>>("summary:service:fallback", "total_requests")
        .await
    {
        fallback_requests = requests.unwrap_or(0);
    }
    if let Ok(amount) = redis
        .get::<&str, Option<f64>>("summary:service:fallback:total_amount")
        .await
    {
        fallback_amount = amount.unwrap_or(0.0);
    }

    let response = SummaryResponse {
        default: SummaryData {
            total_requests: default_requests,
            total_amount: default_amount,
        },
        fallback: SummaryData {
            total_requests: fallback_requests,
            total_amount: fallback_amount,
        },
    };

    Ok(Json(response))
}
