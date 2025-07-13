#![deny(rust_2018_idioms)]

use std::fs;
use std::str::FromStr;

use framebuffer::{Framebuffer, KdMode};
use termion::raw::IntoRawMode;
use thiserror::Error;

use crate::{color::Color, draw::Font, manager::LoginManager};

mod buffer;
mod color;
mod draw;
mod greetd;
mod manager;
mod graphics;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("Error performing buffer operation: {0}")]
    Buffer(#[from] buffer::BufferError),
    #[error("Error performing draw operation: {0}")]
    Draw(#[from] draw::DrawError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Default, Clone)]
struct Module {
    font: Font,
    title_font: Font,
    image_dir: String,
    dialog_horizontal_alignment: f32,
    dialog_vertical_alignment: f32,
    title_horizontal_alignment: f32,
    title_vertical_alignment: f32,
    watermark_horizontal_alignment: f32,
    watermark_vertical_alignment: f32,
    horizontal_alignment: f32,
    vertical_alignment: f32,
    background_start_color: Color,
    background_end_color: Color,
}

impl FromStr for Module {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut module = Module::default();
        for l in s.lines() {
            if l.contains("=") {
                let (key, value) = match &l.split("=").collect::<Vec<&str>>()[..] {
                    &[first, second, ..] => (first, second),
                    _ => unreachable!(),
                };
                let mut v = 0f32;
                if value.starts_with(".") {
                    v = format!("0{}", value).parse().unwrap();
                }
                match key {
                    "Font" => module.font = value.to_string().parse().unwrap(),
                    "TitleFont" => module.title_font = value.to_string().parse().unwrap(),
                    "ImageDir" => module.image_dir = value.to_string(),
                    "DialogHorizontalAlignment" => module.dialog_horizontal_alignment = v,
                    "DialogVerticalAlignment" => module.dialog_vertical_alignment = v,
                    "TitleHorizontalAlignment" => module.title_horizontal_alignment = v,
                    "TitleVerticalAlignment" => module.title_vertical_alignment = v,
                    "HorizontalAlignment" => module.horizontal_alignment = v,
                    "VerticalAlignment" => module.vertical_alignment = v,
                    "WatermarkHorizontalAlignment" => module.watermark_horizontal_alignment = v,
                    "WatermarkVerticalAlignment" => module.watermark_vertical_alignment = v,
                    "BackgroundStartColor" => {
                        module.background_start_color = value.parse().unwrap()
                    }
                    "BackgroundEndColor" => module.background_end_color = value.parse().unwrap(),
                    _ => {}
                }
            }
        }
        Ok(module)
    }
}

#[derive(Default, Clone)]
struct Theme {
    name: String,
    description: Option<String>,
    module: Module,
}

impl FromStr for Theme {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut theme = Theme::default();
        for l in s.lines() {
            if l.contains("=") {
                let (key, value) = match &l.split("=").collect::<Vec<&str>>()[..] {
                    &[first, second, ..] => (first, second),
                    _ => unreachable!(),
                };
                match key {
                    "Name" => theme.name = value.to_string(),
                    "Description" => theme.description = Some(value.to_string()),
                    "ModuleName" => theme.module = s.parse().unwrap(),
                    _ => {}
                }
            }
        }
        Ok(theme)
    }
}

#[derive(Default, Clone)]
struct Config {
    session: Vec<String>,
    theme: Theme,
}

fn parse_theme(theme_file: String) -> Theme {
    let content = fs::read_to_string(theme_file).expect("Unable to read theme file");
    content.parse().unwrap()
}

fn parse_args() -> Config {
    let mut args = std::env::args().skip(1); // skip program name
    let mut config = Config::default();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--session" => {
                if let Some(value) = args.next() {
                    config.session = value.split(" ").map(|s| s.to_string()).collect();
                } else {
                    eprintln!("Expected a value after --session");
                }
            }
            "--theme-file" => {
                if let Some(value) = args.next() {
                    config.theme = parse_theme(value);
                } else {
                    eprintln!("Expected a value after --theme-file");
                }
            }
            _ if arg.starts_with("--") => {
                eprintln!("Unknown flag: {}", arg);
            }
            _ => {
                println!("unknown arg {arg}");
            }
        }
    }

    config
}

fn main() {
    let mut framebuffer = Framebuffer::new("/dev/fb0").expect("unable to open framebuffer device");
    let raw = std::io::stdout()
        .into_raw_mode()
        .expect("unable to enter raw mode");
    Framebuffer::set_kd_mode(KdMode::Graphics).expect("unable to enter graphics mode");
    LoginManager::new(&mut framebuffer, parse_args()).start();
    Framebuffer::set_kd_mode(KdMode::Text).expect("unable to leave graphics mode");
    drop(raw);
}
