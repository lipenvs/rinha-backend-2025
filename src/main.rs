use crate::routes::create_routes;

mod controllers;
mod routes;

#[tokio::main]
async fn main() {
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let client = redis::Client::open(redis_url).unwrap();

    let connection = client
        .get_multiplexed_async_connection()
        .await
        .expect("Falha ao conectar ao Redis");

    let app = create_routes(connection.clone());

    controllers::start_payment_workers(connection.clone()).await;

    println!("API iniciada na porta 3000");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
