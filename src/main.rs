mod cache;
mod model;
mod sc;
mod ui;

use eyre::Result;
use iced::Application;

fn main() -> Result<()> {
    console_subscriber::init();

    std::panic::set_hook(Box::new(|x| log::error!("Panic {x}")));

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
