#[path = "game/app/mod.rs"]
mod app;

fn main() -> Result<(), winit::error::EventLoopError> {
    app::run()
}
