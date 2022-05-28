mod sc;
mod ui;

use eyre::Result;
use iced::pure::Application;

fn main() -> Result<()> {
    // console_subscriber::init();

    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}] {}",
                record.target(),
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Info)
        .chain(std::io::stdout())
        .chain(fern::log_file("output.log")?)
        .apply()?;

    let options: iced::Settings<()> = iced::Settings {
        ..Default::default()
    };

    Ok(ui::App::run(options)?)
}
