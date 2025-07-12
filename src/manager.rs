use std::fs;
use std::io::Read;

use framebuffer::{Framebuffer, KdMode, VarScreeninfo};
use crate::color::Color;

use crate::{buffer, greetd, Config, Error};
const USERNAME_CAP: usize = 64;
const PASSWORD_CAP: usize = 64;

const LAST_USER_USERNAME: &str = "/var/cache/ndlm/lastuser";

// from linux/fb.h
const FB_ACTIVATE_NOW: u32 = 0;
const FB_ACTIVATE_FORCE: u32 = 128;


#[derive(PartialEq, Copy, Clone)]
enum Mode {
    EditingUsername,
    EditingPassword,
}

pub struct LoginManager<'a> {
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
    pub fn new(
        fb: &'a mut Framebuffer,
        screen_size: (u32, u32),
        dimensions: (u32, u32),
        config: Config,
    ) -> Self {
        Self {
            buf: &mut fb.frame,
            device: &fb.device,
            screen_size,
            dimensions,
            mode: Mode::EditingUsername,
            greetd: greetd::GreetD::new(),
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
            (self.screen_size.0 - self.dimensions.0) / 2 + 100,
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
            &mut buf.offset(((self.screen_size.0 / 2) - 100, 32))?,
            &bg,
            &fg,
            &format!("Welcome to {hostname}"),
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

    pub fn start(&mut self) {
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

        match fs::read_to_string(LAST_USER_USERNAME) {
            Ok(user) => {
                self.username = user;
                self.mode = Mode::EditingPassword;
            }
            Err(_) => {}
        };

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
                            match res {
                                Ok(_) => {
                                    let _ = fs::write(LAST_USER_USERNAME, self.username.clone());
                                    return;
                                }
                                Err(_) => {
                                    self.username = String::with_capacity(USERNAME_CAP);
                                    self.password = String::with_capacity(PASSWORD_CAP);
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
