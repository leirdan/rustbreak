use crate::game::game_scene::GameSceneType;
use serde::{Deserialize, Serialize};

/// Represents a message sent to the chat.
///
/// ### Fields
/// - `username`: The name of the user who sent the message;
/// - `content`: The message content itself;
/// - `timestamp`: The exact date and time the message was sent;
/// - `message type`: The type of the message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub username: String,
    pub content: String,
    pub timestamp: String,
    pub message_type: MessageType,
}

/// Models the different types of message that can be sent using the chat.
///
/// ### Types
/// - `UserMessage`: An ordinary message sent by a user;
/// - `SystemNotification`: A notification sent from the server to a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    UserMessage,
    SystemNotification,
}

/// Represents different event signals that can be sent from the server to the client.
///
/// Each variant carries a `String` payload to provide additional context about the event.
/// ### Types
/// - `Error(String)`: An error event with associated data;
/// - `Ok(String)`: A success event with associated data.
/// - `GameScene(GameSceneType`: A game scene event with the scene to be displayed.
/// - `Message(String)`: A message that will be sent;
/// - `Shutdown`: A shutdown event, kills the session.
#[derive(Serialize, Deserialize, Debug)]
pub enum EventSignal {
    Message(ChatMessage),
    Ok(String),
    GameScene(GameSceneType),
    Error(String),
    Shutdown,
}
