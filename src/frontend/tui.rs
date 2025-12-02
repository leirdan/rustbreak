use cursive::{
    Cursive,
    align::HAlign,
    event::{Event, Key},
    theme::{BaseColor, Color, PaletteColor},
    traits::*,
    views::{
        Dialog, DummyView, EditView, EnableableView, LinearLayout, Panel, ScrollView, TextView,
    },
};

use crate::client::add_scroll_callbacks;

/// Builds the entire TUI by calling the necessary setup functions.
///
/// ## Why use this?
/// Calling the multiple setup functions individually in separate places can make some
/// parts of the TUI run on different threads, which can lead to **unexpected behavior**.
/// By using this function, you ensure that all parts of the TUI are available in the **same _tokio_ thread**,
/// and allow changing screens by popping layers without losing any functionality. This function should
/// be called **before** spawning a _tokio_ thread.
///
/// ## How to use:
/// Screen layers are organized in a stack **(LIFO)**, so the order on which each setup function is
/// called matters. The username screen must be on top of the chat screen, so `set_username` must be
/// called after `handle_chat`, as it will be popped as soon as the user sets their username, with
/// the chat screen remaining underneath.
pub fn build_tui(siv: &mut Cursive, input_action: fn(&mut Cursive, String)) {
    handle_chat(siv, input_action);
    set_username(siv, input_action);
    add_scroll_callbacks(siv);
}

/// Handles the chat setup.
///
/// ### Parameters
/// - `siv`: The TUI struct from the Cursive crate;
/// - `input_action`: Function to be run on input box submission.
fn handle_chat(siv: &mut Cursive, input_action: fn(&mut Cursive, String)) {
    // Header to be displayed at the top
    let header = TextView::new(make_header("".to_string()))
        .style(PaletteColor::Secondary)
        .h_align(HAlign::Center)
        .with_name("header");

    // Message area
    let messages = TextView::new("")
        .style(PaletteColor::Secondary)
        .with_name("messages");

    let messages = ScrollView::new(messages)
        .scroll_strategy(cursive::view::ScrollStrategy::StickToBottom)
        .with_name("chat_scroll")
        .min_height(10)
        .full_height()
        .min_width(60)
        .full_width();

    // Input area
    let input = EnableableView::new(
        EditView::new().on_submit(move |s, text| input_action(s, text.to_string())),
    )
    .with_name("chat_input")
    .max_height(3)
    .min_width(50)
    .full_width();

    // Help text for user commands
    let help_text =
        TextView::new("ESC: sair | Enter: enviar | Comandos: /help, /clear, /quit, ...")
            .style(Color::Dark(BaseColor::White));

    let layout = LinearLayout::vertical()
        .child(Panel::new(header))
        .child(
            Dialog::around(messages)
                .title("Chat")
                .title_position(HAlign::Center)
                .full_width(),
        )
        .child(
            Dialog::around(input)
                .title("Input")
                .title_position(HAlign::Center)
                .full_width(),
        )
        .child(Panel::new(help_text).full_width());

    let centered_layout = LinearLayout::horizontal()
        .child(DummyView.full_width())
        .child(layout)
        .child(DummyView.full_width());

    siv.add_fullscreen_layer(centered_layout);

    // Global keybindinds
    siv.add_global_callback(Key::Esc, |s| s.quit());

    siv.add_global_callback('/', |s| {
        s.call_on_name("chat_input", |view: &mut EnableableView<EditView>| {
            if view.is_enabled() {
                view.get_inner_mut().set_content("/");
                // view.get_inner_mut().take_focus(); // Opcional
            }
        });
    });

    siv.add_global_callback(Event::Unknown(vec![127]), |s| {
        s.call_on_name("chat_input", |view: &mut EnableableView<EditView>| {
            if view.is_enabled() {
                let mut content = view.get_inner_mut().get_content().to_string();
                content.pop();
                view.get_inner_mut().set_content(content);
            }
        });
    });

    siv.add_global_callback(Event::Unknown(vec![8]), |s| {
        s.call_on_name("chat_input", |view: &mut EnableableView<EditView>| {
            if view.is_enabled() {
                let content = view.get_inner_mut().get_content().to_string();
                let mut chars = content.chars();
                chars.next();
                view.get_inner_mut().set_content(chars.as_str());
            }
        });
    });

    siv.add_global_callback(Event::CtrlChar('u'), |s| {
        s.call_on_name("chat_input", |view: &mut EnableableView<EditView>| {
            if view.is_enabled() {
                view.get_inner_mut().set_content("");
            }
        });
    });
}

/// Displays a popup to set the username.
///
/// ### Parameters
/// - `siv`: The TUI struct from the Cursive crate;
/// - `input_action`: Function to be run on input box submission.
fn set_username(siv: &mut Cursive, input_action: fn(&mut Cursive, String)) {
    let input = EditView::new()
        .on_submit(move |s, text| {
            let name = text.trim().to_string();
            if name.is_empty() {
                return;
            }

            // Send to the server
            input_action(s, name);

            s.add_layer(
                Dialog::text("Conectando ao Potiguara-Q...").title("Por favor, aguarde..."),
            );
        })
        .style(PaletteColor::Secondary);

    let message = TextView::new("Insira seu nome de jogador abaixo.").with_name("message");

    let tip = TextView::new("Aperte ESC para sair.")
        .style(PaletteColor::Secondary)
        .fixed_height(1)
        .with_name("tip");

    let layout = LinearLayout::vertical()
        .child(DummyView.min_height(1))
        .child(message)
        .child(tip)
        .child(DummyView.min_height(1))
        .child(input);

    siv.add_global_callback(Key::Esc, |s| s.quit());

    // It is not necessary
    // let background = DummyView.full_screen();
    // siv.add_fullscreen_layer(background);

    let popup = Dialog::around(layout)
        .title("Bem vindo ao Rustbreak🦀")
        .title_position(HAlign::Center);

    siv.add_layer(popup);
}

/// Displays a critical error popup with the given message.
/// Prompts the user to quit the application.
///
/// ### Parameters
/// - `siv`: The TUI struct from the Cursive crate;
/// - `error_msg`: The error message to be displayed.
pub fn error_popup(siv: &mut Cursive, error_msg: &str) {
    let message = TextView::new(error_msg).style(PaletteColor::Tertiary);

    let layout = LinearLayout::vertical()
        .child(DummyView.min_height(1))
        .child(message)
        .child(DummyView.min_height(1));

    let popup = Dialog::around(layout)
        .title("ERROR")
        .title_position(HAlign::Center)
        .button("Sair", |siv| siv.quit());

    siv.add_layer(popup);
}

/// Creates the header string with the given username.
pub fn make_header(username: String) -> String {
    format!(r#"⋘ ( *^-^)ρ ⭐ HELLO, {username} ⭐ &(^0^* )⋙"#)
}
