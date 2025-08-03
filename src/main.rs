use crate::routes::create_routes;

mod controllers;
mod routes;

#[tokio::main]
async fn main() {
    let client = redis::Client::open("redis://localhost:6379").unwrap();

    let connection = client
        .get_multiplexed_async_connection()
        .await
        .expect("Falha ao conectar ao Redis");

    let app = create_routes(connection.clone());

    let redis_for_worker = connection.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            println!("Iniciando processamento de novos pagamentos...");
            controllers::new_payments(redis_for_worker.clone()).await;
        }
    });

    let listener = tokio::net::TcpListener::bind("0.0.0.0:9999").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
