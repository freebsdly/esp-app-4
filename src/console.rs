//! Display Log Module
//!
//! This module provides functionality to display log messages on the ST7789 screen.

use crate::spi_lcd::ST7789;
use core::cell::UnsafeCell;
use core::fmt::Write;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as EmbassyMutex;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use heapless::String;

/// A logger that displays messages on the screen
pub struct DisplayLogger<'a, 'd> {
    display: &'a mut ST7789<'d>,
    text_buffer: String<256>,
    line_height: i32,
    max_lines: usize,
    current_line: usize,
}

impl<'a, 'd> DisplayLogger<'a, 'd> {
    /// Create a new DisplayLogger
    pub fn new(display: &'a mut ST7789<'d>) -> Self {
        let height = display.height();
        Self {
            display,
            text_buffer: String::new(),
            line_height: 12,                   // FONT_6X10 height + 2 pixels padding
            max_lines: (height / 12) as usize, // For 240x135 display
            current_line: 0,
        }
    }

    /// Clear the display and reset the logger
    pub fn clear(&mut self) -> Result<(), esp_hal::spi::Error> {
        self.display.clear(Rgb565::WHITE)?;
        self.text_buffer.clear();
        self.current_line = 0;
        Ok(())
    }

    /// Print a line of text to the display
    pub fn println(&mut self, text: &str) -> Result<(), esp_hal::spi::Error> {
        // 检查是否需要清除屏幕（当到达最后一行时）
        if self.current_line >= self.max_lines - 1 {
            // 清除屏幕并重置行计数
            self.display.clear(Rgb565::WHITE)?;
            self.current_line = 0;
        }

        // Draw background for this line
        let line_y = self.current_line as i32 * self.line_height;
        let line_rect = Rectangle::new(
            Point::new(0, line_y),
            Size::new(240, self.line_height as u32),
        );
        let style = PrimitiveStyle::with_fill(Rgb565::WHITE);
        line_rect.into_styled(style).draw(self.display)?;

        // Draw text
        let style = MonoTextStyle::new(&FONT_6X10, Rgb565::BLACK);
        let text = Text::new(text, Point::new(0, line_y + 10), style);
        text.draw(self.display)?;

        // Update current line
        self.current_line = (self.current_line + 1) % self.max_lines;

        Ok(())
    }

    /// Print formatted text to the display
    pub fn print_fmt(&mut self, args: core::fmt::Arguments) -> Result<(), esp_hal::spi::Error> {
        self.text_buffer.clear();
        if let Ok(()) = self.text_buffer.write_fmt(args) {
            // 先获取文本内容，再调用println，避免借用冲突
            let text = self.text_buffer.clone();
            self.println(text.as_str())
        } else {
            self.println("[无法显示文本: 缓冲区溢出]")
        }
    }
}

// 全局显示日志实例
static DISPLAY_LOGGER: EmbassyMutex<
    CriticalSectionRawMutex,
    UnsafeCell<Option<DisplayLogger<'static, 'static>>>,
> = EmbassyMutex::new(UnsafeCell::new(None));

/// 初始化全局显示日志实例
pub fn init_global_logger(logger: DisplayLogger<'static, 'static>) {
    critical_section::with(|_| unsafe {
        *DISPLAY_LOGGER.try_lock().unwrap().get() = Some(logger);
    });
}

/// 在屏幕上打印日志消息
pub fn log_to_display(message: &str) {
    critical_section::with(|_| {
        let logger_ref = unsafe { (*DISPLAY_LOGGER.try_lock().unwrap().get()).as_mut() };

        if let Some(logger) = logger_ref {
            let _ = logger.println(message);
        }
    });
}
