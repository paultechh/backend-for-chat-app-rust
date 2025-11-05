use actix_web::{get, web, HttpRequest, HttpResponse, Error};
use actix_ws::Message as WsMessage;
use futures_util::StreamExt;
use std::collections::HashMap;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::auth::verify_token;
use crate::AppState;

pub struct SessionManager {
    sessions: HashMap<Uuid, Vec<mpsc::UnboundedSender<String>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn add_session(&mut self, room_id: Uuid, tx: mpsc::UnboundedSender<String>) {
        self.sessions.entry(room_id).or_insert_with(Vec::new).push(tx);
    }

    pub fn remove_session(&mut self, room_id: &Uuid, tx: &mpsc::UnboundedSender<String>) {
        if let Some(senders) = self.sessions.get_mut(room_id) {
            senders.retain(|s| !s.same_channel(tx));
            if senders.is_empty() {
                self.sessions.remove(room_id);
            }
        }
    }

    pub fn broadcast(&self, room_id: &Uuid, message: &str) {
        if let Some(senders) = self.sessions.get(room_id) {
            for sender in senders {
                let _ = sender.send(message.to_string());
            }
        }
    }
}

#[get("/ws/{room_id}")]
pub async fn ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    data: web::Data<AppState>,
    room_id: web::Path<Uuid>,
) -> Result<HttpResponse, Error> {
    let token = match req.query_string().split('=').nth(1) {
        Some(t) => t,
        None => return Ok(HttpResponse::Unauthorized().finish()),
    };

    if verify_token(token, &data.jwt_secret).is_err() {
        return Ok(HttpResponse::Unauthorized().finish());
    }

    let room_id = room_id.into_inner();
    let (res, mut session, stream) = actix_ws::handle(&req, stream)?;

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    data.sessions.write().await.add_session(room_id, tx.clone());

    let sessions = data.sessions.clone();
    let redis_client = data.redis.clone();

    actix_web::rt::spawn(async move {
        let mut stream = stream;
        
        // Create Redis pubsub connection
        let client = redis_client;
        let mut pubsub = client.get_async_connection().await.unwrap().into_pubsub();
        pubsub.subscribe(format!("room:{}", room_id)).await.unwrap();
        let mut pubsub_stream = pubsub.on_message();

        loop {
            tokio::select! {
                Some(msg) = stream.next() => {
                    match msg {
                        Ok(WsMessage::Text(text)) => {
                            log::debug!("Received: {}", text);
                        }
                        Ok(WsMessage::Close(_)) => {
                            break;
                        }
                        _ => {}
                    }
                }
                Some(msg) = rx.recv() => {
                    if session.text(msg).await.is_err() {
                        break;
                    }
                }
                Some(msg) = pubsub_stream.next() => {
                    let payload: String = msg.get_payload().unwrap();
                    if session.text(payload).await.is_err() {
                        break;
                    }
                }
            }
        }

        sessions.write().await.remove_session(&room_id, &tx);
        let _ = session.close(None).await;
    });

    Ok(res)
}