//! ST7789 SPI Display Driver
//!
//! This module provides a driver for the ST7789 SPI display controller.
//! It allows initialization and basic control of the display, including
//! setting pixels and filling the screen with colors.

use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::{Rgb565, raw::RawU16},
    prelude::*,
    primitives::Rectangle,
};
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputPin},
    spi::master::Spi,
    Blocking,
};

// Command definitions for ST7789
const CMD_SWRESET: u8 = 0x01; // Software Reset
const CMD_SLPOUT: u8 = 0x11; // Sleep Out
const CMD_PTLON: u8 = 0x12; // Partial Mode ON
const CMD_NORON: u8 = 0x13; // Normal Display Mode ON
const CMD_INVOFF: u8 = 0x20; // Display Inversion OFF
const CMD_INVON: u8 = 0x21; // Display Inversion ON
const CMD_DISPOFF: u8 = 0x28; // Display OFF
const CMD_DISPON: u8 = 0x29; // Display ON
const CMD_CASET: u8 = 0x2A; // Column Address Set
const CMD_RASET: u8 = 0x2B; // Row Address Set
const CMD_RAMWR: u8 = 0x2C; // Memory Write
const CMD_MADCTL: u8 = 0x36; // Memory Data Access Control
const CMD_COLMOD: u8 = 0x3A; // Interface Pixel Format
const CMD_PGC: u8 = 0xE0; // Positive Gamma Correction
const CMD_NGC: u8 = 0xE1; // Negative Gamma Correction
const CMD_FCS: u8 = 0xF0; // Frame rate control
const CMD_CSC: u8 = 0xF1; // Clock Speed Control

// MADCTL register bits
const MADCTL_MY: u8 = 0x80;  // Page Address Order (0: top to bottom, 1: bottom to top)
const MADCTL_MX: u8 = 0x40;  // Column Address Order (0: left to right, 1: right to left)
const MADCTL_MV: u8 = 0x20;  // Page/Column Order (0: normal mode, 1: reverse mode)
const MADCTL_ML: u8 = 0x10;  // Line Address Order (0: LCD refresh from top to bottom, 1: bottom to top)
const MADCTL_RGB: u8 = 0x00; // RGB Order (0: RGB, 1: BGR)
const MADCTL_BGR: u8 = 0x08; // BGR Order (0: RGB, 1: BGR)
const MADCTL_MH: u8 = 0x04;  // Display Data Latch Order (0: LCD refresh from left to right, 1: right to left)

/// Display orientation
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Orientation {
    /// Portrait orientation (normal)
    Portrait,
    /// Portrait orientation (flipped/mirrored)
    PortraitFlipped,
    /// Landscape orientation (rotated 90 degrees)
    Landscape,
    /// Landscape orientation (flipped/mirrored)
    LandscapeFlipped,
}

/// ST7789 display driver
pub struct ST7789<'d> {
    spi: Spi<'d, Blocking>,
    dc: Output<'static>,
    rst: Option<Output<'static>>,
    width: u16,
    height: u16,
    delay: Delay,
}

impl<'d> ST7789<'d> {
    /// Create a new ST7789 driver instance
    pub fn new(
        spi: Spi<'d, Blocking>,
        dc: impl OutputPin + 'static,
        rst: Option<impl OutputPin + 'static>,
        width: u16,
        height: u16,
    ) -> Self {
        let dc = Output::new(dc, Level::Low, Default::default());
        
        let rst = rst.map(|rst| {
            Output::new(rst, Level::High, Default::default())
        });
        
        Self {
            spi,
            dc,
            rst,
            width,
            height,
            delay: Delay::new(),
        }
    }

    /// Initialize the display with default settings
    pub fn init(&mut self) -> Result<(), esp_hal::spi::Error> {
        // 1. 硬件复位（可选但推荐）
        if let Some(rst) = &mut self.rst {
            rst.set_low();
            self.delay.delay_millis(10);
            rst.set_high();
            self.delay.delay_millis(120); // 等待内部初始化完成
        } else {
            // 2. 软件复位（SWRESET）
            self.write_command(CMD_SWRESET, &[])?;
            self.delay.delay_millis(150);
        }

        // 3. 退出睡眠模式（SLPOUT）
        self.write_command(CMD_SLPOUT, &[])?;
        self.delay.delay_millis(120); // 必须等待 5ms，建议 100~120ms

        // 4. 发送初始化序列（关键寄存器配置）
        // 设置内存数据访问控制
        self.write_command(CMD_MADCTL, &[0x00])?; // 正常方向

        // 设置像素格式
        self.write_command(CMD_COLMOD, &[0x55])?; // 16-bit/pixel (RGB565)

        // PORCTRK: Porch Setting
        self.write_command(0xB2, &[0x0C, 0x0C, 0x00, 0x33, 0x33])?;

        // GATECTRL: Gate Control
        self.write_command(0xB7, &[0x35])?;

        // VCOMS: VCOM Setting
        self.write_command(0xBB, &[0x19])?;

        // LCMCTRL: LCM Control
        self.write_command(0xC0, &[0x2C])?;

        // VDVVRHEN: VDV and VRH Command Enable
        self.write_command(0xC2, &[0x01, 0xFF])?;

        // VRHS: VRH Set
        self.write_command(0xC3, &[0x12])?;

        // VDVS: VDV Set
        self.write_command(0xC4, &[0x20])?;

        // FRCTRL2: Frame Rate Control in Normal Mode
        self.write_command(0xC6, &[0x0F])?; // 60Hz

        // PWCTR1: Power Control 1
        self.write_command(0xD0, &[0xA4, 0xA1])?;

        // 正电压伽马校正
        self.write_command(0xE0, &[0xD0, 0x04, 0x0D, 0x11, 0x13, 0x2B, 0x3F, 0x54, 0x4C, 0x18, 0x0D, 0x0B, 0x1F, 0x23])?;

        // 负电压伽马校正
        self.write_command(0xE1, &[0xD0, 0x04, 0x0C, 0x11, 0x13, 0x2C, 0x3F, 0x44, 0x51, 0x2F, 0x1F, 0x1F, 0x20, 0x23])?;

        // 5. 开启显示（DISPON）
        self.write_command(CMD_DISPON, &[])?;
        self.delay.delay_millis(100);
        
        Ok(())
    }

    /// Write a command to the display
    fn write_command(&mut self, cmd: u8, data: &[u8]) -> Result<(), esp_hal::spi::Error> {
        self.dc.set_low(); // Command mode
        self.spi.write(&[cmd])?;
        
        if !data.is_empty() {
            self.dc.set_high(); // Data mode
            self.spi.write(data)?;
        }
        
        Ok(())
    }

    /// Set the address window for drawing
    fn set_address_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) -> Result<(), esp_hal::spi::Error> {
        // CASET: Column Address Set
        self.write_command(
            CMD_CASET,
            &[
                (x0 >> 8) as u8,
                (x0 & 0xFF) as u8,
                (x1 >> 8) as u8,
                (x1 & 0xFF) as u8,
            ],
        )?;
        
        // PASET: Page Address Set
        self.write_command(
            CMD_RASET,
            &[
                (y0 >> 8) as u8,
                (y0 & 0xFF) as u8,
                (y1 >> 8) as u8,
                (y1 & 0xFF) as u8,
            ],
        )?;
        
        Ok(())
    }

    /// Draw a single pixel
    pub fn draw_pixel(&mut self, x: u16, y: u16, color: Rgb565) -> Result<(), esp_hal::spi::Error> {
        if x >= self.width || y >= self.height {
            return Ok(());
        }

        self.set_address_window(x, y, x, y)?;
        self.write_command(CMD_RAMWR, &[])?;
        
        self.dc.set_high(); // Data mode
        let color = RawU16::from(color).into_inner();
        // Prepare color data with byte swapping for RGB565 format
        let color_data = [(color >> 8) as u8, (color & 0xFF) as u8];
        self.spi.write(&color_data)?;
        
        Ok(())
    }

    /// Fill the entire screen with a color
    pub fn fill_screen(&mut self, color: Rgb565) -> Result<(), esp_hal::spi::Error> {
        self.fill_rectangle(0, 0, self.width, self.height, color)
    }

    /// Fill a rectangular region with a color
    pub fn fill_rectangle(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        color: Rgb565,
    ) -> Result<(), esp_hal::spi::Error> {
        if x >= self.width || y >= self.height {
            return Ok(());
        }

        let x1 = x + w - 1;
        let y1 = y + h - 1;

        if x1 >= self.width || y1 >= self.height {
            return Ok(());
        }

        // 6. 设置显示区域（列和行地址）
        self.set_address_window(x, y, x1, y1)?;
        
        // 7. 开始写显存
        self.write_command(CMD_RAMWR, &[])?;
        
        self.dc.set_high(); // Data mode
        
        let color = RawU16::from(color).into_inner();
        let count = w as usize * h as usize;
        
        // Prepare color data with byte swapping for RGB565 format
        let color_data = [(color >> 8) as u8, (color & 0xFF) as u8];
        
        // 使用批量写入优化性能
        // 创建足够大的缓冲区来保存所有像素数据
        let mut buffer = [0u8; 2048]; // 1024个像素的数据
        let mut buffer_index = 0;
        
        for _ in 0..count {
            buffer[buffer_index] = color_data[0];
            buffer[buffer_index + 1] = color_data[1];
            buffer_index += 2;
            
            // 当缓冲区满时，写入数据
            if buffer_index >= buffer.len() {
                self.spi.write(&buffer[..buffer_index])?;
                buffer_index = 0;
            }
        }
        
        // 写入剩余的数据
        if buffer_index > 0 {
            self.spi.write(&buffer[..buffer_index])?;
        }
        
        Ok(())
    }

    /// Get the display width
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Get the display height
    pub fn height(&self) -> u16 {
        self.height
    }

    /// Set display orientation
    pub fn set_orientation(&mut self, orientation: Orientation) -> Result<(), esp_hal::spi::Error> {
        let madctl = match orientation {
            Orientation::Portrait => MADCTL_RGB,
            Orientation::PortraitFlipped => MADCTL_MX | MADCTL_MY | MADCTL_RGB,
            Orientation::Landscape => MADCTL_MV | MADCTL_MX | MADCTL_RGB,
            Orientation::LandscapeFlipped => MADCTL_MV | MADCTL_MY | MADCTL_RGB,
        };
        self.write_command(CMD_MADCTL, &[madctl])
    }
}

// 添加公共访问方法以便在main.rs中能获取SPI实例
impl<'d> ST7789<'d> {
    /// 获取对SPI实例的可变引用
    pub fn spi_mut(&mut self) -> &mut Spi<'d, Blocking> {
        &mut self.spi
    }
    
    /// 释放SPI实例的所有权
    pub fn release_spi(self) -> Spi<'d, Blocking> {
        self.spi
    }
}

impl<'d> DrawTarget for ST7789<'d> {
    type Color = Rgb565;
    type Error = esp_hal::spi::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels.into_iter() {
            self.draw_pixel(coord.x as u16, coord.y as u16, color)?;
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        self.fill_rectangle(
            area.top_left.x as u16,
            area.top_left.y as u16,
            area.size.width as u16,
            area.size.height as u16,
            color,
        )
    }
}

impl<'d> OriginDimensions for ST7789<'d> {
    fn size(&self) -> Size {
        Size::new(self.width as u32, self.height as u32)
    }
}