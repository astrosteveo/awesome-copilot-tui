mod app;
mod domain;
mod io;
mod ui;

use anyhow::Result;

fn main() -> Result<()> {
    app::run()
}
