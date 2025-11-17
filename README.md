# 🎲 Critical One: A Death Rolling Web App Backend

Backend for the World of Warcraft gambling game "Death Rolling," built with Rust, Axum, and Tokio. This project serves as a learning journey for building professional, real-time web applications in Rust.

[![Rust CI](https://github.com/actions/workflows/rust.yml/badge.svg)](https://github.com/YOUR_USERNAME/critical-one/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## ✨ About The Project

Death Rolling is a simple gambling game where players roll against a descending maximum. This backend provides all the necessary logic for player profiles, game lobbies, real-time gameplay via WebSockets, and persistent player statistics.

**The core goal of this project is to build something fun and learn some Rust.**

### Built With

*   [Rust](https://www.rust-lang.org/)
*   [Axum](https://github.com/tokio-rs/axum): Web framework
*   [Tokio](https://tokio.rs/): Asynchronous runtime
*   [PostgreSQL](https://www.postgresql.org/): For persistent player data
*   [Redis](https://redis.io/): For scalable, real-time messaging (Pub/Sub)
*   [SQLx](https://github.com/launchbadge/sqlx): Asynchronous SQL toolkit
*   [Serde](https://serde.rs/): For serialization and deserialization

## 🚀 Getting Started

Follow these instructions to get a local development environment up and running.

### Prerequisites

You will need the following tools installed on your system:
*   **Rust:** Install using `rustup`.
    ```sh
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    ```
*   **Docker:** For running PostgreSQL and Redis databases.
    *   [Install Docker](https://docs.docker.com/get-docker/)
*   **sqlx-cli:** For managing database migrations.
    ```sh
    cargo install sqlx-cli
    ```
*   **websocat:** A command-line tool for testing WebSocket connections.
    ```sh
    # On macOS
    brew install websocat
    # On other systems, see installation instructions
    ```

### Installation & Setup

1.  **Clone the repository:**
    ```sh
    git clone https://github.com/YOUR_USERNAME/critical-one.git
    cd critical-one
    ```

2.  **Start the databases using Docker:**
    ```sh
    docker run --name wow-db -e POSTGRES_PASSWORD=password -p 5432:5432 -d postgres
    docker run --name wow-redis -p 6379:6379 -d redis
    ```

3.  **Create a `.env` file** in the project root and add your database URL:
    ```env
    # .env
    DATABASE_URL="postgres://postgres:password@localhost/postgres"
    ```

4.  **Run the database migrations:**
    ```sh
    sqlx database create
    sqlx migrate run
    ```

5.  **Run the application:**
    ```sh
    cargo run
    ```
    The server will be running on `http://127.0.0.1:3000`.

## 📂 Project Structure

This project follows a `library + binary` structure for clear separation of concerns and high testability.

```
.
├── Cargo.toml
├── migrations/         # SQL database migrations
├── src/
│   ├── main.rs         # Binary Crate: Application entrypoint (starts the server)
│   ├── lib.rs          # Library Crate: Core logic root
│   ├── game.rs         # Pure game logic and rules
│   ├── roller.rs       # Abstraction for random number generation
│   ├── player.rs       # Player profile model and DB interactions
│   ├── db.rs           # Database connection pool setup
│   ├── session.rs      # Game session/lobby management
│   └── handlers.rs     # API and WebSocket request handlers
└── tests/              # Integration tests
```

## 🧪 Running Tests

To run all unit and integration tests, use the following command:
```sh
cargo test
```

## 📡 API Design

The application exposes a REST API for session management and a WebSocket API for real-time gameplay.

### REST API (HTTP)

**Base Path:** `/api`

| Endpoint                  | Method | URL Path                    | Description                                     |
| ------------------------- | ------ | --------------------------- | ----------------------------------------------- |
| **Players & Auth**        |        |                             |                                                 |
| Create Player             | `POST` | `/players`                  | Register a new player profile.                  |
| Login                     | `POST` | `/auth/login`               | Authenticate and receive a JWT.                 |
| Get Player Profile        | `GET`  | `/players/:username`        | View the public stats of any player.            |
| **Game Lobbies**          |        |                             |                                                 |
| Create Game Lobby         | `POST` | `/games`                    | **(Auth)** Creates a new game lobby.            |
| Get Game/Lobby Details    | `GET`  | `/games/:game_id`           | See who is in a lobby or an active game's state.|
| Join Game Lobby           | `POST` | `/games/:game_id/join`      | **(Auth)** Join an existing game lobby.         |
| Start Game                | `POST` | `/games/:game_id/start`     | **(Auth)** Starts the game (host only).         |


### WebSocket API (WS)

Connect to the WebSocket to participate in a game's real-time events.

*   **Connection URL:** `ws://localhost:3000/ws/games/:game_id?token=YOUR_JWT`

#### **Client-to-Server Messages**

| Action      | JSON Message         | Description                      |
|-------------|----------------------|----------------------------------|
| Roll Dice   | `{ "type": "ROLL" }` | Sent by the current player to roll. |

#### **Server-to-Client Broadcasts**

| Event Type          | Example Payload                                            | Description                                          |
|---------------------|------------------------------------------------------------|------------------------------------------------------|
| `PLAYER_JOINED`     | `{ "type": "PLAYER_JOINED", "username": "PlayerB" }`       | A player has connected to the game channel.          |
| `GAME_STARTED`      | `{ "type": "GAME_STARTED", "gameState": { ... } }`         | The host has started the game.                       |
| `TURN_UPDATE`       | `{ "type": "TURN_UPDATE", "currentPlayer": "PlayerA", ... }`| Announces whose turn it is.                        |
| `ROLL_RESULT`       | `{ "type": "ROLL_RESULT", "player": "PlayerA", "roll": 500 }`| The result of a player's roll.                     |
| `GAME_OVER`         | `{ "type": "GAME_OVER", "loser": "PlayerB", "roll": 1 }`   | A player has rolled 1, and the game has ended.     |
| `ERROR`             | `{ "type": "ERROR", "message": "It's not your turn!" }`    | Sent to a specific client for an invalid action.     |
---