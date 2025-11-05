## Chat App Backend (Rust)

A simple real‑time chat backend built with Rust. It exposes REST APIs for auth, rooms and messages, and a WebSocket endpoint for live message updates using Redis pub/sub.

### Tech stack
- **Runtime / Framework**: Rust, Tokio, **Actix Web** (HTTP), **actix-ws** (WebSocket), **actix-cors** (CORS)
- **Database**: PostgreSQL via **sqlx** (async, compile-time checked queries), migrations auto-run at startup
- **Caching / PubSub**: **Redis** for broadcasting new messages to WebSocket clients
- **Auth**: **JWT** via `jsonwebtoken`, password hashing with **bcrypt`
- **Logging / Config**: `env_logger`, `dotenv`
- **Containerization**: `docker-compose` for Postgres and Redis

### Project layout
- `src/auth.rs`: register/login + JWT utilities
- `src/chat.rs`: rooms, list messages, send message (publishes to Redis)
- `src/ws.rs`: WebSocket handler subscribing to Redis pub/sub per room
- `src/models.rs`: shared types used by sqlx and API responses
- `src/main.rs`: server bootstrap, DB/Redis init, route wiring, CORS, migrations
- `migrations/`: SQL migrations (tables + seed rooms)

### API
Base URL: `http://127.0.0.1:8080/api`

- POST `/register` → Register and returns `{ token, user }`
- POST `/login` → Login and returns `{ token, user }`
- GET `/rooms` → List rooms (Authorization: `Bearer <token>`)
- GET `/messages/{room_id}` → Last 50 messages (Authorization)
- POST `/messages` → Send message `{ room_id, content }` (Authorization)

WebSocket:
- `ws://127.0.0.1:8080/api/ws/{room_id}?token=<JWT>`
  - Emits JSON payloads like: `{ "type": "new_message", "room_id": "...", "message": { ... } }`

### Environment variables
You can use a `.env` file in this directory (loaded by `dotenv`):

```
DATABASE_URL=postgres://chat_user:chat_pass@127.0.0.1:5433/chat_db
REDIS_URL=redis://127.0.0.1/
JWT_SECRET=your-secret-key-change-in-production
RUST_LOG=info
```

Defaults are provided in code if not set. The default DB port `5433` matches the docker-compose mapping below.

### Running locally
1) Start infrastructure (Postgres + Redis):
```
docker compose up -d
```

2) Run the server (from this `backend/` folder):
```
cargo run
```

The server listens on `http://127.0.0.1:8080`. Migrations are applied automatically on startup.

### docker-compose
This project includes a compose file mapping Postgres to host port `5433` to avoid conflicts with any local Postgres on `5432`:

```yaml
services:
  postgres:
    image: postgres:15
    environment:
      POSTGRES_USER: chat_user
      POSTGRES_PASSWORD: chat_pass
      POSTGRES_DB: chat_db
    ports:
      - "5433:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data

  redis:
    image: redis:7
    ports:
      - "6379:6379"

volumes:
  pgdata:
```

### Quick test (curl)
Register:
```
curl -s -X POST http://127.0.0.1:8080/api/register \
  -H 'Content-Type: application/json' \
  -d '{"username":"alice","password":"password"}'
```

Login:
```
TOKEN=$(curl -s -X POST http://127.0.0.1:8080/api/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"alice","password":"password"}' | jq -r .token)
```

Rooms:
```
curl -H "Authorization: Bearer $TOKEN" http://127.0.0.1:8080/api/rooms
```

Send message:
```
ROOM_ID=00000000-0000-0000-0000-000000000001
curl -s -X POST http://127.0.0.1:8080/api/messages \
  -H 'Authorization: Bearer '$TOKEN \
  -H 'Content-Type: application/json' \
  -d '{"room_id":"'"$ROOM_ID"'","content":"Hello!"}'
```

### Notes
- CORS is permissive in dev for convenience. Restrict in production.
- `JWT_SECRET` must be set to a strong, private value in production.
- The backend logs the DB connection target at startup to aid debugging.
