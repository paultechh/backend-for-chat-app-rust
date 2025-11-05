use actix_web::{get, post, web, HttpRequest, HttpResponse};
use uuid::Uuid;
use redis::AsyncCommands;

use crate::auth::{extract_token, verify_token};
use crate::models::{Message, Room, SendMessageRequest, WsMessage};
use crate::AppState;

#[post("/messages")]
pub async fn send_message(
    data: web::Data<AppState>,
    req: HttpRequest,
    msg: web::Json<SendMessageRequest>,
) -> HttpResponse {
    let token = match extract_token(&req) {
        Some(t) => t,
        None => return HttpResponse::Unauthorized().json("No token provided"),
    };

    let claims = match verify_token(&token, &data.jwt_secret) {
        Ok(c) => c,
        Err(_) => return HttpResponse::Unauthorized().json("Invalid token"),
    };

    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(id) => id,
        Err(_) => return HttpResponse::BadRequest().json("Invalid user ID"),
    };

    let message_id = Uuid::new_v4();

    let result = sqlx::query_as::<_, Message>(
        r#"
        INSERT INTO messages (id, room_id, user_id, username, content)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING *
        "#
    )
    .bind(message_id)
    .bind(msg.room_id)
    .bind(user_id)
    .bind(&claims.username)
    .bind(&msg.content)
    .fetch_one(&data.db)
    .await;

    match result {
        Ok(message) => {
            // Publish to Redis for WebSocket broadcasting
            if let Ok(mut con) = data.redis.get_multiplexed_async_connection().await {
                let ws_msg = WsMessage {
                    r#type: "new_message".to_string(),
                    room_id: Some(msg.room_id),
                    message: Some(message.clone()),
                };
                let _: Result<(), _> = con.publish(
                    format!("room:{}", msg.room_id),
                    serde_json::to_string(&ws_msg).unwrap()
                ).await;
            }

            HttpResponse::Ok().json(message)
        }
        Err(e) => {
            log::error!("Failed to send message: {}", e);
            HttpResponse::InternalServerError().json("Failed to send message")
        }
    }
}

#[get("/messages/{room_id}")]
pub async fn get_messages(
    data: web::Data<AppState>,
    req: HttpRequest,
    room_id: web::Path<Uuid>,
) -> HttpResponse {
    let token = match extract_token(&req) {
        Some(t) => t,
        None => return HttpResponse::Unauthorized().json("No token provided"),
    };

    if verify_token(&token, &data.jwt_secret).is_err() {
        return HttpResponse::Unauthorized().json("Invalid token");
    }

    let result = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE room_id = $1 ORDER BY created_at DESC LIMIT 50"
    )
    .bind(room_id.into_inner())
    .fetch_all(&data.db)
    .await;

    match result {
        Ok(messages) => HttpResponse::Ok().json(messages),
        Err(e) => {
            log::error!("Failed to get messages: {}", e);
            HttpResponse::InternalServerError().json("Failed to get messages")
        }
    }
}

#[get("/rooms")]
pub async fn get_rooms(
    data: web::Data<AppState>,
    req: HttpRequest,
) -> HttpResponse {
    let token = match extract_token(&req) {
        Some(t) => t,
        None => return HttpResponse::Unauthorized().json("No token provided"),
    };

    if verify_token(&token, &data.jwt_secret).is_err() {
        return HttpResponse::Unauthorized().json("Invalid token");
    }

    let result = sqlx::query_as::<_, Room>(
        "SELECT * FROM rooms ORDER BY name"
    )
    .fetch_all(&data.db)
    .await;

    match result {
        Ok(rooms) => HttpResponse::Ok().json(rooms),
        Err(e) => {
            log::error!("Failed to get rooms: {}", e);
            HttpResponse::InternalServerError().json("Failed to get rooms")
        }
    }
}