#![deny(rust_2018_idioms)]

use std::fs;
use std::io::Read;

use color::Color;
use framebuffer::{Framebuffer, KdMode, VarScreeninfo};
use termion::raw::IntoRawMode;
use thiserror::Error;

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
enum Error {
    #[error("Error performing buffer operation: {0}")]
    Buffer(#[from] buffer::BufferError),
    #[error("Error performing draw operation: {0}")]
    Draw(#[from] draw::DrawError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

struct Target {
    name: String,
    exec: Vec<String>,
}

struct LoginManager<'a> {
    buf: &'a mut [u8],
    device: &'a fs::File,

    headline_font: draw::Font,
    prompt_font: draw::Font,

    screen_size: (u32, u32),
    dimensions: (u32, u32),
    mode: Mode,
    greetd: greetd::GreetD,
    target: Target,

    var_screen_info: &'a VarScreeninfo,
    should_refresh: bool,
}

impl<'a> LoginManager<'a> {
    fn new(
        fb: &'a mut Framebuffer,
        screen_size: (u32, u32),
        dimensions: (u32, u32),
        greetd: greetd::GreetD,
        target: Target,
    ) -> Self {
        Self {
            buf: &mut fb.frame,
            device: &fb.device,
            headline_font: draw::Font::new(&draw::DEJAVUSANS_MONO, 72.0),
            prompt_font: draw::Font::new(&draw::DEJAVUSANS_MONO, 32.0),
            screen_size,
            dimensions,
            mode: Mode::EditingUsername,
            greetd,
            target,
            var_screen_info: &fb.var_screen_info,
            should_refresh: false,
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
        let bg = Color::BLACK;
        buf.memset(&bg);
        self.should_refresh = true;
    }

    fn offset(&self) -> (u32, u32) {
        (
            (self.screen_size.0 - self.dimensions.0) / 2,
            (self.screen_size.1 - self.dimensions.1) / 2,
        )
    }

    fn draw_bg(&mut self, box_color: &Color) -> Result<(), Error> {
        let (x, y) = self.offset();
        let mut buf = buffer::Buffer::new(self.buf, self.screen_size);
        let bg = Color::BLACK;
        let fg = Color::WHITE;

        draw::draw_box(
            &mut buf.subdimensions((x, y, self.dimensions.0, self.dimensions.1))?,
            box_color,
            (self.dimensions.0, self.dimensions.1),
        )?;

        let hostname = hostname::get()?.to_string_lossy().into_owned();

        self.headline_font.auto_draw_text(
            &mut buf.offset(((self.screen_size.0 / 2) - 300, 32))?,
            &bg,
            &fg,
            &format!("Welcome to {hostname}"),
        )?;

        self.headline_font.auto_draw_text(
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

        self.prompt_font.auto_draw_text(
            &mut buf
                .subdimensions((x, y, self.dimensions.0, self.dimensions.1))?
                .offset((256, 64))?,
            &bg,
            &username_color,
            "username:",
        )?;

        self.prompt_font.auto_draw_text(
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
        let bg = Color::BLACK;
        if redraw {
            buf.memset(&bg);
        }

        self.prompt_font
            .auto_draw_text(&mut buf, &bg, &Color::WHITE, username)?;

        self.should_refresh = true;

        Ok(())
    }

    fn draw_password(&mut self, password: &str, redraw: bool) -> Result<(), Error> {
        let (x, y) = self.offset();
        let (x, y) = (x + 416, y + 104);
        let dim = (self.dimensions.0 - 416 - 32, 32);

        let mut buf = buffer::Buffer::new(self.buf, self.screen_size);
        let mut buf = buf.subdimensions((x, y, dim.0, dim.1))?;
        let bg = Color::BLACK;
        if redraw {
            buf.memset(&bg);
        }

        let mut stars = "".to_string();
        for _ in 0..password.len() {
            stars += "*";
        }

        self.prompt_font
            .auto_draw_text(&mut buf, &bg, &Color::WHITE, &stars)?;

        self.should_refresh = true;

        Ok(())
    }

    fn goto_next_mode(&mut self) {
        self.mode = match self.mode {
            Mode::EditingUsername => Mode::EditingPassword,
            Mode::EditingPassword => Mode::EditingUsername,
        }
    }

    fn greeter_loop(&mut self) {
        let mut username = String::with_capacity(USERNAME_CAP);
        let mut password = String::with_capacity(PASSWORD_CAP);
        let mut last_username_len = username.len();
        let mut last_password_len = password.len();
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
            if username.len() != last_username_len {
                self.draw_username(&username, username.len() < last_username_len)
                    .expect("unable to draw username prompt");
                last_username_len = username.len();
            }
            if password.len() != last_password_len {
                self.draw_password(&password, password.len() < last_password_len)
                    .expect("unable to draw username prompt");
                last_password_len = password.len();
            }
            if last_mode != self.mode {
                self.draw_bg(&Color::GRAY)
                    .expect("unable to draw background");
                last_mode = self.mode;
            }

            if had_failure {
                self.draw_bg(&Color::GRAY)
                    .expect("unable to draw background");
                had_failure = false;
            }

            match read_byte() as char {
                '\x15' | '\x0B' => match self.mode {
                    // ctrl-k/ctrl-u
                    Mode::EditingUsername => username.clear(),
                    Mode::EditingPassword => password.clear(),
                },
                '\x03' | '\x04' => {
                    // ctrl-c/ctrl-D
                    username.clear();
                    password.clear();
                    self.greetd.cancel();
                    return;
                }
                '\x7F' => match self.mode {
                    // backspace
                    Mode::EditingUsername => {
                        username.pop();
                    }
                    Mode::EditingPassword => {
                        password.pop();
                    }
                },
                '\t' => self.goto_next_mode(),
                '\r' => match self.mode {
                    Mode::EditingUsername => {
                        if !username.is_empty() {
                            self.mode = Mode::EditingPassword;
                        }
                    }
                    Mode::EditingPassword => {
                        if password.is_empty() {
                            username.clear();
                            self.mode = Mode::EditingUsername;
                        } else {
                            self.draw_bg(&Color::YELLOW)
                                .expect("unable to draw background");
                            let res =
                                self.greetd
                                    .login(username, password, self.target.exec.to_owned());
                            username = String::with_capacity(USERNAME_CAP);
                            password = String::with_capacity(PASSWORD_CAP);
                            match res {
                                Ok(_) => return,
                                Err(_) => {
                                    self.draw_bg(&Color::RED)
                                        .expect("unable to draw background");
                                    self.mode = Mode::EditingUsername;
                                    self.greetd.cancel();
                                    had_failure = true;
                                }
                            }
                        }
                    }
                },
                v => match self.mode {
                    Mode::EditingUsername => username.push(v as char),
                    Mode::EditingPassword => password.push(v as char),
                },
            }
            self.refresh();
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut framebuffer = Framebuffer::new("/dev/fb0").expect("unable to open framebuffer device");

    let w = framebuffer.var_screen_info.xres;
    let h = framebuffer.var_screen_info.yres;

    let raw = std::io::stdout()
        .into_raw_mode()
        .expect("unable to enter raw mode");

    let _ = Framebuffer::set_kd_mode(KdMode::Graphics).expect("unable to enter graphics mode");

    let greetd = greetd::GreetD::new();

    let mut lm = LoginManager::new(
        &mut framebuffer,
        (w, h),
        (1024, 168),
        greetd,
        Target {
            name: "default".to_string(),
            exec: args[1..].to_vec(),
        },
    );

    lm.clear();
    lm.draw_bg(&Color::GRAY).expect("unable to draw background");
    lm.greeter_loop();
    let _ = Framebuffer::set_kd_mode(KdMode::Text).expect("unable to leave graphics mode");
    drop(raw);
}
