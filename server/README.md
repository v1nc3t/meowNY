# Meowny Server

Spring Boot REST API for **Meowny**, a personal expense tracker. Handles user authentication, transactions, budgets, categories, and recurring transaction templates.

- **Java 21** · **Spring Boot 3.4** · **PostgreSQL** · **JWT** · **JPA**

---

## Table of contents

- [Prerequisites](#prerequisites)
- [Quick start](#quick-start)
- [Configuration](#configuration)
- [Running the application](#running-the-application)
- [Testing](#testing)
- [API overview](#api-overview)
- [Authentication](#authentication)
- [Project structure](#project-structure)
- [Development](#development)
- [Troubleshooting](#troubleshooting)

---

## Prerequisites

| Tool | Version |
|------|---------|
| Java JDK | 21+ |
| Maven | 3.9+ (or use the included `./mvnw` wrapper) |
| PostgreSQL | 16+ recommended |
| Docker | Optional — required only for Testcontainers integration tests |

Verify your setup:

```bash
java -version
./mvnw -version
psql --version
```

---

## Quick start

### 1. Clone the repository

```bash
git clone <repository-url>
cd meowNY
```

### 2. Create environment file

Configuration lives in a **repo-root** `.env` file (one level above `server/`). Spring Boot loads it via `spring.config.import` in `application.properties`.

```bash
cp .env.example .env   # if an example file exists; otherwise create .env manually
```

Minimum `.env` contents:

```env
# Database
DB_HOST=localhost
DB_PORT=5432
DB_NAME=meowny_db
DB_USER=postgres
DB_PASSWORD=your_postgres_password

# Auth — generate strong random values (see Configuration)
JWT_SECRET=replace_with_at_least_32_bytes
APP_SECURITY_PEPPER=replace_with_random_string
```

Generate secrets:

```bash
openssl rand -base64 32   # JWT_SECRET (must be ≥ 32 bytes)
openssl rand -base64 24   # APP_SECURITY_PEPPER
```

### 3. Create the database

```bash
psql -U postgres -c "CREATE DATABASE meowny_db;"
```

### 4. Run the server

```bash
cd server
./mvnw spring-boot:run
```

The API listens on **http://localhost:8080**.

Health check (no auth required):

```bash
curl http://localhost:8080/actuator/health
```

---

## Configuration

All environment variables are read from the repo-root `.env` file unless overridden on the command line.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DB_HOST` | No | `localhost` | PostgreSQL host |
| `DB_PORT` | No | `5432` | PostgreSQL port |
| `DB_NAME` | No | `meowny_db` | Database name |
| `DB_USER` | No | `postgres` | Database user |
| `DB_PASSWORD` | **Yes** | — | Database password |
| `JWT_SECRET` | **Yes** | — | HMAC signing key (≥ 32 bytes) |
| `JWT_EXPIRATION_MS` | No | `86400000` | Token lifetime in ms (24 h) |
| `APP_SECURITY_PEPPER` | **Yes** | — | Server-side password pepper for Argon2id |
| `APP_CORS_ORIGINS` | No | `http://localhost:5173` | Comma-separated allowed CORS origins |

Additional settings are in `src/main/resources/application.properties`:

| Property | Default | Notes |
|----------|---------|-------|
| `server.port` | `8080` | HTTP port |
| `spring.jpa.hibernate.ddl-auto` | `update` | Auto-update schema in dev; use `validate` + migrations in production |

> **Security:** Never commit `.env` to version control. It is listed in `.gitignore`.

---

## Running the application

### Development (hot reload optional)

```bash
cd server
./mvnw spring-boot:run
```

### Production JAR

```bash
cd server
./mvnw clean package -DskipTests
java -jar target/server-0.0.1-SNAPSHOT.jar
```

### With explicit environment variables

```bash
JWT_SECRET='...' APP_SECURITY_PEPPER='...' DB_PASSWORD='...' ./mvnw spring-boot:run
```

---

## Testing

### Unit & integration tests

Repository integration tests spin up a **PostgreSQL Testcontainer** automatically — Docker must be running.

```bash
cd server
./mvnw test
```

Run a single test class:

```bash
./mvnw test -Dtest=TransactionServiceTest
```

### Static analysis

```bash
./mvnw checkstyle:check
```

---

## API overview

Base URL: `http://localhost:8080/api/v1`

All endpoints except `/auth/**` and `/actuator/health` require a valid JWT.

| Resource | Path | Methods |
|----------|------|---------|
| Auth | `/auth/register`, `/auth/login` | `POST` |
| User profile | `/users/me` | `GET`, `PUT`, `DELETE` |
| Transactions | `/transactions`, `/transactions/{id}` | `GET`, `POST`, `PUT`, `DELETE` |
| Categories | `/categories`, `/categories/{id}` | `GET`, `POST`, `PUT`, `DELETE` |
| Category groups | `/category-groups`, `/category-groups/{id}` | `GET`, `POST`, `PUT`, `DELETE` |
| Budgets | `/budgets`, `/budgets/{id}` | `GET`, `POST`, `PUT`, `DELETE` |
| Recurring transactions | `/recurring-transactions`, `/recurring-transactions/{id}` | `GET`, `POST`, `PUT`, `DELETE` |

List endpoints return data for the **authenticated user only**. Do not send `userId` in request bodies — ownership is derived from the JWT.

---

## Authentication

### Register

```bash
curl -X POST http://localhost:8080/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "firstName": "Jane",
    "lastName": "Doe",
    "email": "jane@example.com",
    "username": "jane",
    "password": "securepass1234"
  }'
```

**Response** `201 Created`:

```json
{ "token": "<jwt>", "type": "Bearer" }
```

### Login

```bash
curl -X POST http://localhost:8080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{ "username": "jane", "password": "securepass1234" }'
```

### Authenticated requests

```bash
curl http://localhost:8080/api/v1/users/me \
  -H "Authorization: Bearer <jwt>"
```

### Security details

- Passwords hashed with **Argon2id** + server-side pepper
- Stateless **JWT** sessions (default 24 h expiry)
- Per-user resource ownership enforced on every protected endpoint
- Password minimum length: **12 characters**

---

## Project structure

```
server/
├── src/main/java/com/meowny/server/
│   ├── config/          # Security, CORS, sanitization
│   ├── controller/      # REST endpoints
│   ├── dto/             # Request/response records
│   ├── entity/          # JPA entities
│   ├── exception/       # Custom exceptions & global handler
│   ├── repository/      # Spring Data JPA repositories
│   ├── security/        # JWT filter, password encoder, current user
│   └── service/         # Business logic
├── src/main/resources/
│   └── application.properties
├── src/test/java/       # Unit & integration tests
├── pom.xml
└── mvnw                 # Maven wrapper
```

---

## Development

### Conventions

- **Controllers** are thin — validate input, delegate to services, return DTOs.
- **Services** own business rules and enforce that the authenticated user owns the resource being accessed.
- **DTOs** use Java `record` types with Jakarta Bean Validation annotations.
- **Entities** are never returned directly from the API.
- User-supplied text fields are sanitized via `HtmlSanitizationDeserializer` where applicable.

### Adding a new endpoint

1. Define request/response DTOs under `dto/`.
2. Add service method — use `CurrentUserService.getCurrentUser()` for ownership; never trust client-supplied `userId`.
3. Add controller method under the appropriate `/api/v1/...` path.
4. Write unit tests in `src/test/java/.../service/`.
5. If touching persistence, add or extend a repository integration test.

### Local frontend integration

The default CORS origin is `http://localhost:5173` (Vite). To allow another origin:

```env
APP_CORS_ORIGINS=http://localhost:5173,http://localhost:3000
```

### Useful commands

```bash
# Compile without running tests
./mvnw compile

# Run with debug logging
./mvnw spring-boot:run -Dlogging.level.com.meowny=DEBUG

# Package
./mvnw clean package

# Dependency tree
./mvnw dependency:tree
```

### Git workflow

Work on feature branches off `main`. Keep commits focused — for example, separate infrastructure, auth endpoints, and authorization changes.

---

## Troubleshooting

| Problem | Likely cause | Fix |
|---------|--------------|-----|
| `app.jwt.secret must be at least 32 bytes` | Missing or short `JWT_SECRET` | Set `JWT_SECRET` in `.env` (use `openssl rand -base64 32`) |
| `app.security.pepper must not be blank` | Missing `APP_SECURITY_PEPPER` | Add pepper to `.env` |
| `Connection refused` to PostgreSQL | DB not running or wrong credentials | Start PostgreSQL; verify `DB_*` in `.env` |
| `401 Unauthorized` on protected routes | Missing or expired JWT | Login again; send `Authorization: Bearer <token>` |
| `403 Forbidden` | Valid token but insufficient access | Expected for cross-user access attempts |
| Integration tests fail | Docker not running | Start Docker daemon before `./mvnw test` |
| `No plugin found for prefix 'spring-boot'` | Ran Maven from repo root | Run commands from the `server/` directory |

---

## License

See the root repository for license information.
