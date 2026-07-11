//! Desktop and Web entry point. Both targets render the same Dioxus component tree.

mod api;
mod app;
mod views;

fn main() {
    dioxus::launch(app::App);
}
