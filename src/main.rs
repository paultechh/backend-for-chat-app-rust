use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpServer};
use sqlx::postgres::PgPoolOptions;
use redis::Client as RedisClient;
use std::sync::Arc;
use tokio::sync::RwLock;

mod auth;
mod chat;
mod db;
mod models;
mod ws;

pub struct AppState {
    db: sqlx::PgPool,
    redis: RedisClient,
    sessions: Arc<RwLock<ws::SessionManager>>,
    jwt_secret: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://chat_user:chat_pass@127.0.0.1:5433/chat_db".to_string());
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://127.0.0.1/".to_string());
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "your-secret-key-change-in-production".to_string());

    log::info!("Attempting to connect to database: postgres://chat_user:***@127.0.0.1:5433/chat_db");
    
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .map_err(|e| {
            log::error!("Database connection error: {:?}", e);
            log::error!("Connection string used: {}", database_url.replace("chat_pass", "***"));
            e
        })
        .expect("Failed to connect to Postgres");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Failed to run migrations");

    let redis = RedisClient::open(redis_url.as_str()).expect("Failed to connect to Redis");

    let sessions = Arc::new(RwLock::new(ws::SessionManager::new()));

    let app_state = web::Data::new(AppState {
        db,
        redis,
        sessions,
        jwt_secret,
    });

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    log::info!("Starting server at http://0.0.0.0:{}", port);

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .app_data(app_state.clone())
            .wrap(middleware::Logger::default())
            .wrap(cors)
            .service(
                web::scope("/api")
                    .service(auth::register)
                    .service(auth::login)
                    .service(chat::send_message)
                    .service(chat::get_messages)
                    .service(chat::get_rooms)
                    .service(ws::ws_handler)
            )
    })
    .bind(("0.0.0.0", port))?
    .run()
    .await
}