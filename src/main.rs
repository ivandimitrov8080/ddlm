#![deny(rust_2018_idioms)]

use std::fs;
use std::io::Read;
use std::str::FromStr;

use color::Color;
use framebuffer::{Framebuffer, KdMode, VarScreeninfo};
use termion::raw::IntoRawMode;
use thiserror::Error;

use crate::draw::Font;

const USERNAME_CAP: usize = 64;
const PASSWORD_CAP: usize = 64;

// from linux/fb.h
const FB_ACTIVATE_NOW: u32 = 0;
const FB_ACTIVATE_FORCE: u32 = 128;

mod buffer;
mod color;
mod draw;
mod greetd;

#[derive(PartialEq, Copy, Clone)]
enum Mode {
    EditingUsername,
    EditingPassword,
}

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

struct LoginManager<'a> {
    buf: &'a mut [u8],
    device: &'a fs::File,

    screen_size: (u32, u32),
    dimensions: (u32, u32),
    mode: Mode,
    greetd: greetd::GreetD,
    config: Config,

    var_screen_info: &'a VarScreeninfo,
    should_refresh: bool,
    username: String,
    password: String,
}

impl<'a> LoginManager<'a> {
    fn new(
        fb: &'a mut Framebuffer,
        screen_size: (u32, u32),
        dimensions: (u32, u32),
        greetd: greetd::GreetD,
        config: Config,
    ) -> Self {
        Self {
            buf: &mut fb.frame,
            device: &fb.device,
            screen_size,
            dimensions,
            mode: Mode::EditingUsername,
            greetd,
            config,
            var_screen_info: &fb.var_screen_info,
            should_refresh: false,
            username: String::with_capacity(USERNAME_CAP),
            password: String::with_capacity(PASSWORD_CAP),
        }
    }

    fn refresh(&mut self) {
        if self.should_refresh {
            self.should_refresh = false;
            let mut screeninfo = self.var_screen_info.clone();
            screeninfo.activate |= FB_ACTIVATE_NOW | FB_ACTIVATE_FORCE;
            Framebuffer::put_var_screeninfo(self.device, &screeninfo)
                .expect("Failed to refresh framebuffer");
        }
    }

    fn clear(&mut self) {
        let mut buf = buffer::Buffer::new(self.buf, self.screen_size);
        let bg = self.config.theme.module.background_start_color;
        buf.memset(&bg);
        self.should_refresh = true;
    }

    fn offset(&self) -> (u32, u32) {
        (
            (self.screen_size.0 - self.dimensions.0) / 2,
            (self.screen_size.1 - self.dimensions.1) / 2,
        )
    }

    fn draw_bg(&mut self) -> Result<(), Error> {
        let (x, y) = self.offset();
        let mut buf = buffer::Buffer::new(self.buf, self.screen_size);
        let bg = self.config.theme.module.background_start_color;
        let fg = Color::WHITE;

        let hostname = hostname::get()?.to_string_lossy().into_owned();

        let mut title_font = self.config.theme.module.title_font.clone();
        let mut prompt_font = self.config.theme.module.font.clone();

        title_font.auto_draw_text(
            &mut buf.offset(((self.screen_size.0 / 2) - 300, 32))?,
            &bg,
            &fg,
            &format!("Welcome to {hostname}"),
        )?;

        title_font.auto_draw_text(
            &mut buf
                .subdimensions((x, y, self.dimensions.0, self.dimensions.1))?
                .offset((32, 24))?,
            &bg,
            &fg,
            "Login",
        )?;

        let (username_color, password_color) = match self.mode {
            Mode::EditingUsername => (Color::YELLOW, Color::WHITE),
            Mode::EditingPassword => (Color::WHITE, Color::YELLOW),
        };

        prompt_font.auto_draw_text(
            &mut buf
                .subdimensions((x, y, self.dimensions.0, self.dimensions.1))?
                .offset((256, 64))?,
            &bg,
            &username_color,
            "username:",
        )?;

        prompt_font.auto_draw_text(
            &mut buf
                .subdimensions((x, y, self.dimensions.0, self.dimensions.1))?
                .offset((256, 104))
                .unwrap(),
            &bg,
            &password_color,
            "password:",
        )?;

        self.should_refresh = true;

        Ok(())
    }

    fn draw_username(&mut self, username: &str, redraw: bool) -> Result<(), Error> {
        let (x, y) = self.offset();
        let (x, y) = (x + 416, y + 64);
        let dim = (self.dimensions.0 - 416 - 32, 32);

        let mut buf = buffer::Buffer::new(self.buf, self.screen_size);
        let mut buf = buf.subdimensions((x, y, dim.0, dim.1))?;
        let mut prompt_font = self.config.theme.module.font.clone();
        let bg = self.config.theme.module.background_start_color;
        if redraw {
            buf.memset(&bg);
        }

        prompt_font.auto_draw_text(&mut buf, &bg, &Color::WHITE, username)?;

        self.should_refresh = true;

        Ok(())
    }

    fn draw_password(&mut self, password: &str, redraw: bool) -> Result<(), Error> {
        let (x, y) = self.offset();
        let (x, y) = (x + 416, y + 104);
        let dim = (self.dimensions.0 - 416 - 32, 32);

        let mut buf = buffer::Buffer::new(self.buf, self.screen_size);
        let mut buf = buf.subdimensions((x, y, dim.0, dim.1))?;
        let mut prompt_font = self.config.theme.module.font.clone();
        let bg = self.config.theme.module.background_start_color;
        if redraw {
            buf.memset(&bg);
        }

        let mut stars = "".to_string();
        for _ in 0..password.len() {
            stars += "*";
        }

        prompt_font.auto_draw_text(&mut buf, &bg, &Color::WHITE, &stars)?;

        self.should_refresh = true;

        Ok(())
    }

    fn goto_next_mode(&mut self) {
        self.mode = match self.mode {
            Mode::EditingUsername => Mode::EditingPassword,
            Mode::EditingPassword => Mode::EditingUsername,
        }
    }

    fn redraw(&mut self) {
        self.draw_bg().expect("unable to draw background");
        self.draw_username(&self.username.clone(), true)
            .expect("unable to draw username prompt");
        self.draw_password(&self.password.clone(), true)
            .expect("unable to draw password prompt");
    }

    fn draw(&mut self) {
        self.clear();
        let mut last_mode = self.mode;
        let mut had_failure = false;
        let stdin_handle = std::io::stdin();
        let stdin_lock = stdin_handle.lock();
        let mut stdin_bytes = stdin_lock.bytes();

        fn quit() -> u8 {
            Framebuffer::set_kd_mode(KdMode::Text).expect("unable to leave graphics mode");
            std::process::exit(1);
        }
        let mut read_byte = || stdin_bytes.next().and_then(Result::ok).unwrap_or_else(quit);

        loop {
            if last_mode != self.mode {
                last_mode = self.mode;
            }
            if had_failure {
                had_failure = false;
            }
            self.redraw();

            match read_byte() as char {
                '\x15' | '\x0B' => match self.mode {
                    // ctrl-k/ctrl-u
                    Mode::EditingUsername => self.username.clear(),
                    Mode::EditingPassword => self.password.clear(),
                },
                '\x03' | '\x04' => {
                    // ctrl-c/ctrl-D
                    self.username.clear();
                    self.password.clear();
                    self.greetd.cancel();
                    return;
                }
                '\x7F' => match self.mode {
                    // backspace
                    Mode::EditingUsername => {
                        self.username.pop();
                    }
                    Mode::EditingPassword => {
                        self.password.pop();
                    }
                },
                '\t' => self.goto_next_mode(),
                '\r' => match self.mode {
                    Mode::EditingUsername => {
                        if !self.username.is_empty() {
                            self.mode = Mode::EditingPassword;
                        }
                    }
                    Mode::EditingPassword => {
                        if self.password.is_empty() {
                            self.username.clear();
                            self.mode = Mode::EditingUsername;
                        } else {
                            let res = self.greetd.login(
                                self.username.clone(),
                                self.password.clone(),
                                self.config.session.clone(),
                            );
                            self.username = String::with_capacity(USERNAME_CAP);
                            self.password = String::with_capacity(PASSWORD_CAP);
                            match res {
                                Ok(_) => return,
                                Err(_) => {
                                    self.mode = Mode::EditingUsername;
                                    self.greetd.cancel();
                                    had_failure = true;
                                }
                            }
                        }
                    }
                },
                v => match self.mode {
                    Mode::EditingUsername => self.username.push(v as char),
                    Mode::EditingPassword => self.password.push(v as char),
                },
            }
            self.refresh();
        }
    }
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
    let config = parse_args();
    let mut framebuffer = Framebuffer::new("/dev/fb0").expect("unable to open framebuffer device");

    let w = framebuffer.var_screen_info.xres;
    let h = framebuffer.var_screen_info.yres;

    let raw = std::io::stdout()
        .into_raw_mode()
        .expect("unable to enter raw mode");

    let _ = Framebuffer::set_kd_mode(KdMode::Graphics).expect("unable to enter graphics mode");

    let greetd = greetd::GreetD::new();

    let mut lm = LoginManager::new(&mut framebuffer, (w, h), (1024, 168), greetd, config);

    lm.draw();
    let _ = Framebuffer::set_kd_mode(KdMode::Text).expect("unable to leave graphics mode");
    drop(raw);
}
