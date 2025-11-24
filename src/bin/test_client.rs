use critical_one::game::{GameId, PlayerId};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, timeout, Duration, Instant};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

#[derive(Debug, Deserialize)]
struct CreateGameResponse {
    game_id: GameId,
}

#[derive(Debug, Serialize)]
struct CreateGameRequest {
    host_id: Option<PlayerId>,
}

#[derive(Debug, Serialize)]
struct JoinGameRequest {
    player_id: Option<PlayerId>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "SCREAMING_SNAKE_CASE")]
enum ClientMessage {
    Roll,
}

async fn spawn_game_connection(
    game_id: GameId,
    player_id: PlayerId,
    name: String,
    is_host: bool,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
    let ws_base = "ws://127.0.0.1:3000/ws/game";
    let url_str = format!("{}/{}?player_id={}", ws_base, game_id, player_id);

    let handle = tokio::spawn(async move {
        let (ws_stream, _) = connect_async(&url_str).await.expect("failed to connect");
        let (mut write, mut read) = ws_stream.split();

        println!("....[{name}] Connected!");
        if is_host {
            // Wait for everyone to be ready
            sleep(Duration::from_secs(2)).await;

            println!("....[{name}] Rolling dice...");
            let msg = serde_json::to_string(&ClientMessage::Roll).unwrap();

            if let Err(e) = write.send(Message::Text(msg.into())).await {
                eprintln!("....[{name}] Failed to send roll: {}", e);
            }
        }

        let listen_duration = Duration::from_secs(5);
        let start = Instant::now();

        while let Ok(Some(msg)) = timeout(Duration::from_secs(5), read.next()).await {
            match msg {
                Ok(msg) => {
                    if let Message::Text(text) = msg {
                        println!("....[{name} RX] {}", text);
                    } else if let Message::Close(_) = msg {
                        println!("....[{name}] Server closed connection");
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("....[{name}] Error reading message: {}", e);
                    break;
                }
            }

            if start.elapsed() > listen_duration {
                break;
            }
        }
        println!("....[{name}] Disconnecting");
    });

    Ok(handle)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup IDs
    let host_id = PlayerId::new();
    let guest_id = PlayerId::new();
    let client = reqwest::Client::new();
    let base_url = "http://127.0.0.1:3000";

    println!("--- 🎲 CRITICAL ONE TEST CLIENT ---");
    println!("Host ID:  {}", host_id);
    println!("Guest ID: {}", guest_id);

    println!("\n[1] Creating Game...");
    let resp = client
        .post(format!("{}/game", base_url))
        .json(&CreateGameRequest {
            host_id: Some(host_id),
        })
        .send()
        .await?;

    let resp_text = resp.text().await?;
    let create_data: CreateGameResponse = serde_json::from_str(&resp_text)?;
    let game_id = create_data.game_id;

    println!("Success! Game ID: {}", game_id);

    println!("\n[2] Guest Joining...");
    let join_resp = client
        .post(format!("{}/game/{}/join", base_url, game_id))
        .json(&JoinGameRequest {
            player_id: Some(guest_id),
        })
        .send()
        .await?;

    if !join_resp.status().is_success() {
        eprintln!("Guest join failed: {}", join_resp.text().await?);
        return Ok(());
    }
    println!("Success! Guest joined.");

    println!("\n[3] Connecting WebSockets...");

    let host_handle = spawn_game_connection(game_id, host_id, "Host".to_string(), true).await?;
    let guest_handle = spawn_game_connection(game_id, guest_id, "Guest".to_string(), false).await?;

    // Wait for both tasks to complete their loops
    let _ = tokio::join!(host_handle, guest_handle);

    println!("\n--- Test Complete ---");
    Ok(())
}
