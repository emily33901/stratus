#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

mod cache;
mod downloader;
mod model;
mod sc;
mod ui_egui;
mod ui_iced;

use std::error::Error;

use iced::Application;

fn main() -> std::result::Result<(), Box<dyn Error>> {
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
        .chain(std::io::stderr())
        .apply()?;

    if true {
        let options: iced::Settings<()> = iced::Settings {
            ..Default::default()
        };

        Ok(ui_iced::App::run(options)?)
    } else {
        let options = eframe::NativeOptions::default();
        Ok(eframe::run_native(
            "Stratus",
            options,
            Box::new(|_cc| Box::<ui_egui::App>::default()),
        )?)
    }
}
