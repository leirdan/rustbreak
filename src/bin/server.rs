use chrono::Local;
use rustbreak::common::{
    formatting::*,
    messages::{ChatMessage, EventSignal, MessageType},
    shared::*,
};
use rustbreak::game::game_scene::GameSceneType;
use rustbreak::game::game_session::{
    GameEvent, GameSession, UpdateResult, MAX_PLAYERS_PER_SESSION,
};
use std::collections::HashMap;
use std::time::Duration;
use std::{error::Error, sync::Arc};
use tokio::sync::mpsc;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::{broadcast, Mutex},
};
use uuid::Uuid;

pub struct SessionEntry {
    pub session: Arc<Mutex<GameSession>>,
    pub broadcast: broadcast::Sender<String>,
    pub event_channel: mpsc::Sender<GameEvent>,
}

impl SessionEntry {
    pub fn new(
        session: Arc<Mutex<GameSession>>,
        broadcast: broadcast::Sender<String>,
        event_channel: mpsc::Sender<GameEvent>,
    ) -> Self {
        Self {
            session,
            broadcast,
            event_channel,
        }
    }
}
pub type ServerSessions = Arc<Mutex<HashMap<Uuid, SessionEntry>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let listener = TcpListener::bind(format!("{ADDRESS}:{PORT}")).await?;

    let sessions: ServerSessions = Arc::new(Mutex::new(HashMap::<Uuid, SessionEntry>::new()));

    welcome_message();

    loop {
        let (socket, addr) = listener.accept().await?;

        println!(
            "┌─[{}] {GREEN}Nova conexão!{RESET}",
            Local::now().format("%H:%M:%S")
        );
        println!("└─ Endereço: {BLUE}{addr}{RESET}");

        let sessions_clone = Arc::clone(&sessions);
        tokio::spawn(async move { handle_connection(socket, sessions_clone, addr).await });
    }
}

fn welcome_message() {
    println!("╔════════════════════════════════════════╗");
    println!("║                                        ║");
    println!(
        "║    {BLUE}SERVER RUNNING ON {CYAN}{}:{}{RESET}    ║",
        ADDRESS, PORT
    );
    println!("║    {YELLOW}Press Ctrl+C to shutdown{RESET}            ║");
    println!("║                                        ║");
    println!("╚════════════════════════════════════════╝");
}

async fn handle_connection(
    mut socket: TcpStream,
    sessions: ServerSessions,
    addr: core::net::SocketAddr,
) {
    let (reader, mut writer) = socket.split();
    let mut reader = BufReader::new(reader);
    let mut username = String::new();
    let mut party_id: Option<Uuid> = None;
    let mut broadcast_receiver: Option<broadcast::Receiver<String>> = None;
    let mut broadcast_sender: Option<broadcast::Sender<String>> = None;
    let mut event_sender: Option<mpsc::Sender<GameEvent>> = None;
    let event_receiver: Option<mpsc::Receiver<GameEvent>> = None;

    // LOGIN + PLAYER REGISTRY
    loop {
        username.clear();

        match reader.read_line(&mut username).await {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) => {
                eprintln!("{RED}ERRO lendo de {addr}: {e}{RESET}");
                return;
            }
        }

        let username_trim = username.trim().to_string();
        if username_trim.is_empty() {
            continue;
        }

        // Scope to manipulate sessions
        {
            let sessions_arc_clone_for_spawn = Arc::clone(&sessions);

            let mut sessions_map = sessions.lock().await;
            let mut available_id: Option<Uuid> = None;

            // Search for a available party
            for (id, entry) in sessions_map.iter() {
                let session = entry.session.lock().await;
                if session.party.len() < MAX_PLAYERS_PER_SESSION {
                    available_id = Some(*id);
                    break;
                }
            }

            let res: Result<
                (
                    Uuid,
                    broadcast::Sender<String>,
                    broadcast::Receiver<String>,
                    mpsc::Sender<GameEvent>,
                ),
                String,
            > = match available_id {
                // Creates a new session with own communication channel because an available party wasn't found
                None => {
                    let (broadcast_sender_local, broadcast_receiver_local) =
                        broadcast::channel::<String>(128);
                    let (event_sender_local, event_receiver_local) =
                        mpsc::channel::<GameEvent>(128);

                    let new_session = Arc::new(Mutex::new(GameSession::new()));
                    let party_id_local = new_session.lock().await.id;

                    sessions_map.insert(
                        party_id_local,
                        SessionEntry::new(
                            new_session.clone(),
                            broadcast_sender_local.clone(),
                            event_sender_local.clone(),
                        ),
                    );

                    let entry = sessions_map.get_mut(&party_id_local).unwrap();

                    let mut session_lock = entry.session.lock().await;
                    match session_lock.add_player(&username_trim) {
                        Ok(_) => {
                            drop(session_lock);

                            let sessions_clone_for_task = sessions_arc_clone_for_spawn;
                            let event_clone = event_sender_local.clone();
                            let broadcast_clone = broadcast_sender_local.clone();
                            let new_session_clone = new_session.clone();

                            // Init a new game_loop thread for this party
                            tokio::spawn(async move {
                                game_loop(
                                    new_session_clone,
                                    broadcast_clone,
                                    event_clone,
                                    event_receiver_local,
                                    party_id_local,
                                    sessions_clone_for_task,
                                )
                                .await;
                            });

                            Ok((
                                party_id_local,
                                broadcast_sender_local,
                                broadcast_receiver_local,
                                event_sender_local,
                            ))
                        }
                        Err(err) => Err(err),
                    }
                }
                // Found a session with an available party
                Some(available_session_id) => {
                    let entry = sessions_map.get_mut(&available_session_id).unwrap();
                    let mut session_lock = entry.session.lock().await;
                    match session_lock.add_player(&username_trim) {
                        Ok(_) => Ok((
                            session_lock.id,
                            entry.broadcast.clone(),
                            entry.broadcast.subscribe(),
                            entry.event_channel.clone(),
                        )),
                        Err(err) => Err(err),
                    }
                }
            };

            match res {
                Ok((id, b_sender, b_receiver, e_sender)) => {
                    party_id = Some(id);
                    broadcast_sender = Some(b_sender);
                    broadcast_receiver = Some(b_receiver);
                    event_sender = Some(e_sender);
                }
                Err(err) => {
                    let error_signal = EventSignal::Error(err.clone());
                    let json = serde_json::to_string(&error_signal).unwrap();

                    if writer.write_all(json.as_bytes()).await.is_err()
                        || writer.write_all(b"\n").await.is_err()
                    {
                        return;
                    }
                    continue;
                }
            }
        }

        let ok_signal = EventSignal::Ok(username.clone());
        let json = serde_json::to_string(&ok_signal).unwrap();

        if writer.write_all(json.as_bytes()).await.is_err()
            || writer.write_all(b"\n").await.is_err()
        {
            let mut sessions_map = sessions.lock().await;
            if let Some(entry) = sessions_map.get_mut(&party_id.unwrap()) {
                entry.session.lock().await.remove_player(&username);
            }
            return;
        }
        username = username_trim.to_string();

        break;
    }

    // These asserts are just to ensure a safe unwrap below.
    assert!(party_id.is_some());
    assert!(event_sender.is_some());
    assert!(broadcast_sender.is_some());
    assert!(broadcast_receiver.is_some());
    let party_id = party_id.unwrap();
    let event_channel = event_sender.unwrap();
    let broadcast_sender = broadcast_sender.unwrap();
    let mut broadcast_receiver = broadcast_receiver.unwrap();

    // Communicates the enter of a new player
    let _ = event_channel
        .send(GameEvent::PlayerJoined(username.clone()))
        .await;

    let mut line = String::new();

    // This loop observes every line on reader or on our broadcast channel, searching for new messages
    // incoming to server, and sends them to other players!
    loop {
        tokio::select! {
            result = reader.read_line(&mut line) => {
                if result.unwrap() == 0 { break; }
                let content = line.trim().to_string();
                line.clear();

                if content.starts_with("/answer ") {
                    let answer = content["/answer ".len()..].trim().to_string();
                    const VALID_OPTIONS: [&str; 4] = ["a", "b", "c", "d"];
                    if !VALID_OPTIONS.contains(&answer.to_lowercase().as_str()) {
                        let error_msg = ChatMessage {
                            username: "ERROR".to_string(),
                            content: format!("Alternativa '{}' inválida! Por favor responda com A, B, C ou D.", answer),
                            timestamp: get_time(),
                            message_type: MessageType::SystemNotification,
                        };

                        if let Ok(json) = serde_json::to_string(&error_msg) {
                            let _ = writer.write_all(json.as_bytes()).await;
                            let _ = writer.write_all(b"\n").await;
                        }

                        continue;
                    }

                    let _ = event_channel.send(GameEvent::PlayerAnswer { username: username.clone(), answer }).await;
                    continue;
                }
                else {
                    send_user_msg(&username, content.clone(), &broadcast_sender).await;
                }
            }

            result = broadcast_receiver.recv() => {
                if let Ok(message) = result {
                    if writer.write_all(message.as_bytes()).await.is_err() {
                        break;
                    }
                    if writer.write_all(b"\n").await.is_err() {
                        break;
                    }
                }
            }
        }
    }

    // The player disconnected!
    {
        let mut sessions_map = sessions.lock().await;
        if let Some(entry) = sessions_map.get_mut(&party_id) {
            entry.session.lock().await.remove_player(&username);

            let leave_msg = format!("{} deixou o chat!", username);
            send_server_msg(leave_msg, &entry.broadcast).await;
        }
    }

    println!(
        "├─[{}] {YELLOW}'{}' desconectou.{RESET}",
        get_time(),
        username
    );
}

async fn game_loop(
    session: Arc<Mutex<GameSession>>,
    broadcast_channel: broadcast::Sender<String>,
    event_channel: mpsc::Sender<GameEvent>,
    mut event_receiver: mpsc::Receiver<GameEvent>,
    party_id: Uuid,
    sessions: ServerSessions,
) {
    // Listens to every game event on event_receiver channel and acts properly to handle each one,
    // either updating the game, sending event signals to the client, etc..
    while let Some(event) = event_receiver.recv().await {
        match event {
            GameEvent::PlayerJoined(player) => {
                let join_msg = format!("{} entrou no chat!", player);
                send_server_msg(join_msg, &broadcast_channel).await;
                println!(
                    "├─[{}] {GREEN}'{}' se juntou ao chat!{RESET}",
                    get_time(),
                    player
                );

                let mut s = session.lock().await;
                if s.party.len() == MAX_PLAYERS_PER_SESSION && !s.has_started {
                    s.begin_game();

                    if let GameSceneType::Prelude(_) = &s.current_scene_state {
                        emit_scene_signal(&s.current_scene_state, &broadcast_channel).await;
                        s.next_scene();

                        if let GameSceneType::Normal(scene) = &s.current_scene_state {
                            let scene_clone = scene.clone();
                            emit_scene_signal(
                                &GameSceneType::Normal(scene_clone),
                                &broadcast_channel,
                            )
                            .await;
                        }
                        drop(s);
                        continue;
                    }
                }
            }
            GameEvent::PlayerAnswer { username, answer } => {
                let mut s = session.lock().await;
                match s.update(GameEvent::PlayerAnswer {
                    username: username.clone(),
                    answer,
                }) {
                    UpdateResult::Advance(feedback) => {
                        send_server_msg(feedback, &broadcast_channel).await;
                        let _ = event_channel.send(GameEvent::AdvanceTurn).await;
                    }
                    UpdateResult::Continue(Some(answer)) => {
                        send_server_msg(
                            format!("{} escolheu a resposta {}", username, answer),
                            &broadcast_channel,
                        )
                        .await;
                    }
                    UpdateResult::Continue(None) => {}
                    UpdateResult::GameOver(error_msg) => {
                        let _ = event_channel
                            .send(GameEvent::GameEnding(UpdateResult::GameOver(error_msg)))
                            .await;
                    }
                    UpdateResult::EndGame(scene_msg) => {
                        let _ = event_channel
                            .send(GameEvent::GameEnding(UpdateResult::EndGame(scene_msg)))
                            .await;
                    }
                }
            }
            GameEvent::AdvanceTurn => {
                let mut s = session.lock().await;
                s.next_scene();
                match &s.current_scene_state {
                    GameSceneType::Normal(_) => {
                        emit_scene_signal(&s.current_scene_state, &broadcast_channel).await;
                    }
                    _ => {}
                }
                drop(s);
            }
            GameEvent::GameEnding(result) => match result {
                UpdateResult::GameOver(msg) => {
                    game_over(msg, &broadcast_channel).await;
                    {
                        let mut sessions_map = sessions.lock().await;
                        if sessions_map.remove(&party_id).is_some() {
                            println!(
                                "├─[{}] {YELLOW}Sessão {} removida da memória (GameOver).{RESET}",
                                get_time(),
                                party_id
                            );
                        }
                    }

                    break;
                }
                UpdateResult::EndGame(msg) => {
                    end_game(msg, &broadcast_channel).await;
                    {
                        let mut sessions_map = sessions.lock().await;
                        if sessions_map.remove(&party_id).is_some() {
                            println!(
                                "├─[{}] {YELLOW}Sessão {} removida da memória (EndGame).{RESET}",
                                get_time(),
                                party_id
                            );
                        }
                    }

                    break;
                }
                _ => {}
            },
        }
    }

    println!(
        "├─[{}] {CYAN}game_loop() terminado para sessão {RESET}{}",
        get_time(),
        party_id
    );
}

// Utility functions below

async fn end_game(scene_msg: String, broadcast: &broadcast::Sender<String>) {
    send_server_msg(scene_msg, &broadcast).await;
    tokio::time::sleep(Duration::from_secs(5)).await;

    let shutdown_signal = EventSignal::Shutdown;
    let json = serde_json::to_string(&shutdown_signal).unwrap();
    let _ = broadcast.send(json);
}

async fn game_over(error_msg_scene: String, broadcast: &broadcast::Sender<String>) {
    send_server_msg(error_msg_scene, &broadcast).await;
    tokio::time::sleep(Duration::from_secs(2)).await;

    let game_over_msg = "Andando pelos corredores do IMD, vocês recebem uma notificação no terminal. Quando o abrem, leem a seguinte mensagem: \n 'Caros ajudantes, vocês se provaram ineficientes para a tarefa a qual lhes foi passada. Infelizmente, lhes falta conhecimento do nosso sistema para que consigam nos ajudar. Desejo que prosperem no seu desenvolvimento enquanto programadores e em outra vida sejam capazes de me ajudar. \nCrab Guardian'. Vocês saem cabisbaixos pela entrada do IMD sabendo que falharam na missão, esperando que outras pessoas mais experientes sejam capazes de consertar este caos.".into();
    send_server_msg(game_over_msg, &broadcast).await;

    let shutdown_signal = EventSignal::Shutdown;
    let json = serde_json::to_string(&shutdown_signal).unwrap();
    let _ = broadcast.send(json);
}

async fn emit_scene_signal(scene: &GameSceneType, broadcast: &broadcast::Sender<String>) {
    let scene_signal = EventSignal::GameScene(scene.clone());
    let scene_json = serde_json::to_string(&scene_signal).unwrap();
    let _ = broadcast.send(format!("{}", scene_json));
}

async fn send_server_msg(msg: String, broadcast: &broadcast::Sender<String>) {
    let msg = ChatMessage {
        username: SYSTEM_NAME.into(),
        content: msg,
        timestamp: get_time(),
        message_type: MessageType::SystemNotification,
    };
    let msg = serde_json::to_string(&msg).unwrap();
    let _ = broadcast.send(msg);
}

async fn send_user_msg(username: &String, msg: String, broadcast: &broadcast::Sender<String>) {
    let msg = ChatMessage {
        username: username.clone(),
        content: msg,
        timestamp: get_time(),
        message_type: MessageType::UserMessage,
    };

    let msg = serde_json::to_string(&msg).unwrap();
    let _ = broadcast.send(msg);
}
