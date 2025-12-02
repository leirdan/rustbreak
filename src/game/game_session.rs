use crate::game::game_scene::{GameScene, GameSceneType};
use crate::game::player::Player;
use chrono::Local;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use uuid::Uuid;

pub const MAX_PLAYERS_PER_SESSION: usize = 3;

/// A enum that the consumer of the session can use to handle each events of the game
/// - `PlayerJoined(String)`: this represents the entrance of a new player in a party. The consumer can start the session based on it;
/// - `PlayerAnswer {username: String, answer: String }`: this represents an answer from one player.
///     The consumer can compute the round results based on it;
/// - `AdvanceTurn`: this represents the game must advance to next round. The consumer can send messages
///     based on it and advance the game;
/// - `GameEnding(UpdateResult)`: this represents a game ending, that can be a normal ending or
///     a game over (the UpdateResult that indicates it). The consumer then can send properly events to handle
///     this.
pub enum GameEvent {
    PlayerJoined(String),
    PlayerAnswer { username: String, answer: String },
    AdvanceTurn,
    GameEnding(UpdateResult),
}

/// An enum that represents a result from the update method of the game
/// - `Advance(String)`: indicates to the consumer that the game should advance to the next round
/// - `Continue(Option<String>)`: if the String is Some, indicates the game should keep in this round but
///     print this message (usually indicates the answer of one player). If is None, it's a generic answer.
/// - `EndGame(String)`: indicates to the consumer that the game is ended (normally) and it should print the final message
/// - `GameOver(String)`: indicates to the consumer that the party failed on its mission and the game is over.
pub enum UpdateResult {
    Advance(String),
    Continue(Option<String>),
    EndGame(String),
    GameOver(String),
}

#[derive(Clone)]
pub struct GameSession {
    pub id: Uuid,
    pub current_scene_state: GameSceneType,
    pub party: HashSet<Player>,
    pub has_started: bool,
    answers: HashMap<String, bool>,
    remaining_answers: i8,
    remaining_scenes: VecDeque<String>,
}

impl GameSession {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            current_scene_state: GameSceneType::Prelude(String::new()),
            party: HashSet::new(),
            answers: HashMap::new(),
            has_started: false,
            remaining_answers: 3,
            remaining_scenes: vec![
                "scene_1".into(),
                "scene_2".into(),
                "scene_3".into(),
                "scene_4".into(),
                "scene_5".into(),
                "scene_6".into(),
                "scene_7".into(),
                "final_scene".into(),
            ]
            .into(),
        }
    }

    pub fn add_player(&mut self, username: &String) -> Result<(), String> {
        if self.contains(&username) {
            return Err(format!("Player já registrado: {}", username));
        } else if self.party.len() >= MAX_PLAYERS_PER_SESSION {
            return Err(format!("Party já está cheia: {}", username));
        }

        if self.party.insert(Player::new(username.clone())) {
            Ok(())
        } else {
            Err("Não é possível adicionar o mesmo jogador duas vezes".to_string())
        }
    }

    pub fn contains(&self, username: &String) -> bool {
        self.party.iter().any(|p| p.username == *username)
    }

    pub fn remove_player(&mut self, username: &String) {
        self.party.retain(|p| p.username != *username);
    }

    /// Method that computes a game event and return UpdateResults to the consumer.
    pub fn update(&mut self, event: GameEvent) -> UpdateResult {
        // Prevents updates if the game hasn't started
        if !self.has_started {
            return UpdateResult::Continue(None);
        }

        match event {
            GameEvent::PlayerAnswer { username, answer } => {
                let scene = match &self.current_scene_state {
                    GameSceneType::Normal(scene) => scene,
                    _ => return UpdateResult::Continue(None),
                };

                // Computes the answer of the current player
                let correct = answer
                    .trim()
                    .eq_ignore_ascii_case(&scene.options.id_correct);

                self.answers.insert(username.clone(), correct);

                if self.answers.len() < self.party.len() {
                    return UpdateResult::Continue(Some(answer));
                }

                // At this point, every player has already answered, so compute the round results.

                let correct_count = self.answers.values().filter(|v| **v).count();
                let wrong_count = self.party.len() - correct_count;

                self.answers.clear();
                let text_result: String;

                if correct_count >= wrong_count {
                    text_result = scene.success_msg.clone();
                } else {
                    text_result = scene.error_msg.clone();
                    self.remaining_answers -= 1;
                }

                // If they missed all they answers, it's a game over :(
                if self.remaining_answers <= 0 {
                    return UpdateResult::GameOver(scene.error_msg.clone());
                }

                // But if they reached a state where all scenes were completed, it's a game ending :)
                // and the game return the ending (can be a good or bad ending)
                if self.remaining_scenes.is_empty() {
                    return UpdateResult::EndGame(text_result);
                }

                let mut count_result = format!(
                    "{} jogadores acertaram e {} erraram! Vocês ainda têm {} tentativa(s)! \n",
                    correct_count, wrong_count, self.remaining_answers
                );

                count_result.push_str(&text_result);
                UpdateResult::Advance(count_result)
            }
            GameEvent::PlayerJoined(_) => UpdateResult::Continue(None),
            _ => UpdateResult::Continue(None),
        }
    }

    pub fn next_scene(&mut self) {
        match self.remaining_scenes.pop_front() {
            None => {}
            Some(scene) => {
                let _ = self.load_scene(scene.as_str());
            }
        }
    }

    pub fn get_scene_json(&self) -> Option<String> {
        match &self.current_scene_state {
            GameSceneType::Normal(scene) => serde_json::to_string(scene).ok(),
            _ => None,
        }
    }

    fn load_scene(&mut self, path: &str) -> Result<(), &'static str> {
        let mut full_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        full_path.push("data");
        full_path.push(format!("{}.json", path));

        let file = File::open(&full_path).map_err(|_| "Falhou ao carregar a cena")?;
        let reader = BufReader::new(file);

        let game_scene: GameScene =
            serde_json::from_reader(reader).map_err(|_| "Falhou ao deserializar a cena")?;
        self.current_scene_state = GameSceneType::Normal(game_scene);
        Ok(())
    }

    fn get_prelude_text(&self) -> String {
        let current_date = Local::now().format("%d/%m/%Y %H:%M").to_string();
        format!(
            r#"
Iniciando conexão com quantum.imd.ufrn.br...
Protocolo TELNET handshake... OK.
...Conexão segura estabelecida.

BEM-VINDOS, Investigadores.

(A tela pisca brevemente...)
(A luz do terminal falha...)

[PANIC: unexpected kernel trap]
[SEGFAULT @0x00ffd19a]
[FATAL: recursion detected in non-recursive function]
[STACK OVERFLOW PROTECTOR: DISARMED]

(A tela estabiliza novamente.)

Data Estelar: {}
Status do Sistema: **CRÍTICO**
Local: Instituto Metrópole Digital (IMD), UFRN.

O orgulho do IMD, o supercomputador 'Potiguara-Q', foi ativado esta manhã. Escrito inteiramente em Rust para garantir segurança e performance quântica, ele era a promessa de uma nova era…

Porém...

A promessa falhou.

Não do jeito que vocês devem estar pensando. O 'Potiguara-Q' não 'crashou'. Ele... 'compilou'. A realidade do campus da UFRN foi tratada como seu código-fonte, e ele encontrou 'bugs'.

Agora, o computador está ativamente tentando 'corrigir' a realidade, causando anomalias catastróficas. O sistema está em caos.

Uma sub-rotina de segurança de baixo nível, o 'Crab Guardian', conseguiu contatar vocês. Ele identificou seus terminais como pertencentes a usuários que entendem a Lógica por trás da Linguagem.

Sem mais enrolação, vamos para vossa missão:

Entrar no sistema.
Encontrar as anomalias.
E forçar um CONSENSO.

Vocês devem 'corrigir o código' da realidade, juntos.

Mas cuidado.
O 'Crab Guardian' detectou... 'interferência'. As anomalias não parecem totalmente acidentais…

Enfim... O conhecimento, a discussão e o consenso são suas únicas armas.

STATUS DA SESSÃO: JOGADORES CONECTADOS: {}

O chat de vocês está aberto. Discutam.
O Saguão espera.
Boa sorte. Vocês vão precisar."#,
            current_date,
            self.party.len()
        )
    }

    pub fn begin_game(&mut self) {
        if self.has_started {
            return;
        }

        self.has_started = true;
        self.current_scene_state = GameSceneType::Prelude(self.get_prelude_text());

        println!(
            "Sessão {} iniciou com {} jogadores.",
            self.id,
            self.party.len()
        );
    }
}
