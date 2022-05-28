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

    #[cfg(target_os = "")]
    {
        let sc = sc::new()?;

        let song = sc
            .song(Id::Url("https://soundcloud.com/iammindsight/anotherone"))
            .await?;

        let user = sc.user(Id::Url("https://soundcloud.com/f1ssi0n")).await?;

        println!("{:#?}", song.read());
        println!("{:#?}", user.read());

        let playlist = sc
            .playlist(Id::Url("https://soundcloud.com/f1ssi0n/sets/june-1"))
            .await?;

        for song in &playlist.read().songs {
            println!("{:#?}", song);
        }
    }
    let options: iced::Settings<()> = iced::Settings {
        ..Default::default()
    };

    Ok(ui::App::run(options)?)
}
