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

    let app = create_routes(connection);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:9999").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
