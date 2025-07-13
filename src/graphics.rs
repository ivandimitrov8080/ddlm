//! graphics.rs
// A p5.js-style graphics API over DRM dumb buffers

use drm::buffer::{Buffer, DrmFourcc};
use drm::control::dumbbuffer::DumbBuffer;
use drm::control::Device as ControlDevice;
use drm::Device as BasicDevice;
use std::f64::consts::PI;
use std::fs::File;
use std::os::fd::AsFd;
use std::os::unix::io::{AsRawFd, RawFd};

pub struct Graphics<'a> {
    pub width: usize,
    pub height: usize,
    fb: DumbBuffer,
    buf: &'a mut [u8],
    card: Card,
    stride: usize,
    bg_color: Color,
    fg_color: Color,
}

#[derive(Clone, Copy)]
pub struct Color(pub u8, pub u8, pub u8);

struct Card(File);
impl AsRawFd for Card {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}
impl AsFd for Card {
    fn as_fd(&self) -> std::os::unix::prelude::BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl BasicDevice for Card {}
impl ControlDevice for Card {}

impl Graphics<'_> {
    pub fn new(path: &str, width: usize, height: usize) -> Self {
        let file = File::options().read(true).write(true).open(path).unwrap();
        let card = Card(file);

        let mut fb = card
            .create_dumb_buffer((width as u32, height as u32), DrmFourcc::Big_endian, 32)
            .unwrap();
        let mut handle = card.map_dumb_buffer(&mut fb).unwrap();
        let buf =
            unsafe { std::slice::from_raw_parts_mut(handle.as_mut_ptr(), fb.size() as usize) };

        Self {
            width,
            height,
            fb,
            buf,
            card,
            stride: fb.pitch() as usize,
            bg_color: Color(0, 0, 0),
            fg_color: Color(255, 255, 255),
        }
    }

    pub fn set_color(&mut self, color: Color) {
        self.fg_color = color;
    }

    pub fn clear(&mut self) {
        for chunk in self.buf.chunks_exact_mut(4) {
            let Color(r, g, b) = self.bg_color;
            chunk[0] = b;
            chunk[1] = g;
            chunk[2] = r;
            chunk[3] = 0xff;
        }
    }

    pub fn put_pixel(&mut self, x: isize, y: isize) {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return;
        }
        let i = (y as usize * self.stride) + (x as usize * 4);
        if i + 3 < self.buf.len() {
            let Color(r, g, b) = self.fg_color;
            self.buf[i] = b;
            self.buf[i + 1] = g;
            self.buf[i + 2] = r;
            self.buf[i + 3] = 0xff;
        }
    }

    pub fn draw_circle(&mut self, cx: isize, cy: isize, radius: isize) {
        for a in 0..360 {
            let rad = a as f64 * PI / 180.0;
            let x = cx + (radius as f64 * rad.cos()) as isize;
            let y = cy - (radius as f64 * rad.sin()) as isize;
            self.put_pixel(x, y);
        }
    }

    pub fn draw_line(&mut self, x0: isize, y0: isize, x1: isize, y1: isize) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        let mut x = x0;
        let mut y = y0;
        loop {
            self.put_pixel(x, y);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    pub fn draw_point(&mut self, x: isize, y: isize, radius: isize) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy <= radius * radius {
                    self.put_pixel(x + dx, y + dy);
                }
            }
        }
    }

    pub fn present(&mut self) {
        std::fs::write("framebuffer.raw", self.buf).unwrap();
    }
}
