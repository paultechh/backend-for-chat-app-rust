use actix_web::{post, web, HttpResponse, HttpRequest};
use bcrypt::{hash, verify, DEFAULT_COST};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use uuid::Uuid;

use crate::models::{AuthResponse, Claims, LoginRequest, RegisterRequest, User, UserInfo};
use crate::AppState;

#[post("/register")]
pub async fn register(
    data: web::Data<AppState>,
    req: web::Json<RegisterRequest>,
) -> HttpResponse {
    let password_hash = match hash(&req.password, DEFAULT_COST) {
        Ok(h) => h,
        Err(_) => return HttpResponse::InternalServerError().json("Failed to hash password"),
    };

    let user_id = Uuid::new_v4();

    let result = sqlx::query_as::<_, User>(
        "INSERT INTO users (id, username, password_hash) VALUES ($1, $2, $3) RETURNING *"
    )
    .bind(user_id)
    .bind(&req.username)
    .bind(&password_hash)
    .fetch_one(&data.db)
    .await;

    match result {
        Ok(user) => {
            let token = create_token(&user.id.to_string(), &user.username, &data.jwt_secret);
            HttpResponse::Ok().json(AuthResponse {
                token,
                user: UserInfo {
                    id: user.id,
                    username: user.username,
                },
            })
        }
        Err(_) => HttpResponse::BadRequest().json("Username already exists"),
    }
}

#[post("/login")]
pub async fn login(
    data: web::Data<AppState>,
    req: web::Json<LoginRequest>,
) -> HttpResponse {
    let result = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE username = $1"
    )
    .bind(&req.username)
    .fetch_one(&data.db)
    .await;

    match result {
        Ok(user) => {
            if verify(&req.password, &user.password_hash).unwrap_or(false) {
                let token = create_token(&user.id.to_string(), &user.username, &data.jwt_secret);
                HttpResponse::Ok().json(AuthResponse {
                    token,
                    user: UserInfo {
                        id: user.id,
                        username: user.username,
                    },
                })
            } else {
                HttpResponse::Unauthorized().json("Invalid credentials")
            }
        }
        Err(_) => HttpResponse::Unauthorized().json("Invalid credentials"),
    }
}

fn create_token(user_id: &str, username: &str, secret: &str) -> String {
    let expiration = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(24))
        .expect("valid timestamp")
        .timestamp() as usize;

    let claims = Claims {
        sub: user_id.to_owned(),
        username: username.to_owned(),
        exp: expiration,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_ref()),
    )
    .unwrap()
}

pub fn verify_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_ref()),
        &Validation::default(),
    )
    .map(|data| data.claims)
}

pub fn extract_token(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("Authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|s| s.to_string())
}