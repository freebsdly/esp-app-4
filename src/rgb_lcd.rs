//! RGB LCD Driver for ESP32
//!
//! This module provides a driver for RGB LCD displays connected directly to the ESP32's LCD peripheral.
//! It supports various screen sizes and handles the low-level configuration of the LCD controller.

use core::marker::PhantomData;
use esp_hal::{gpio::OutputPin, lcd_cam::LcdCam, peripheral::Peripheral};
use esp_hal::spi::Error as LcdError;

/// GPIO definitions for RGB LCD
pub const GPIO_LCD_DE: Option<i8> = Some(4); // DE signal pin
pub const GPIO_LCD_PCLK: Option<i8> = Some(5); // Pixel clock pin

// Red data pins
pub const GPIO_LCD_R3: Option<i8> = Some(45);
pub const GPIO_LCD_R4: Option<i8> = Some(48);
pub const GPIO_LCD_R5: Option<i8> = Some(47);
pub const GPIO_LCD_R6: Option<i8> = Some(21);
pub const GPIO_LCD_R7: Option<i8> = Some(14);

// Green data pins
pub const GPIO_LCD_G2: Option<i8> = Some(10);
pub const GPIO_LCD_G3: Option<i8> = Some(9);
pub const GPIO_LCD_G4: Option<i8> = Some(46);
pub const GPIO_LCD_G5: Option<i8> = Some(3);
pub const GPIO_LCD_G6: Option<i8> = Some(8);
pub const GPIO_LCD_G7: Option<i8> = Some(18);

// Blue data pins
pub const GPIO_LCD_B3: Option<i8> = Some(17);
pub const GPIO_LCD_B4: Option<i8> = Some(16);
pub const GPIO_LCD_B5: Option<i8> = Some(15);
pub const GPIO_LCD_B6: Option<i8> = Some(7);
pub const GPIO_LCD_B7: Option<i8> = Some(6);

// Commonly used RGB565 color values
/// White color: R=31, G=63, B=31
pub const WHITE: u16 = 0xFFFF;
/// Black color: R=0, G=0, B=0
pub const BLACK: u16 = 0x0000;
/// Red color: R=31, G=0, B=0
pub const RED: u16 = 0xF800;
/// Green color: R=0, G=63, B=0
pub const GREEN: u16 = 0x07E0;
/// Blue color: R=0, G=0, B=31
pub const BLUE: u16 = 0x001F;
/// Magenta color: R=31, G=0, B=31
pub const MAGENTA: u16 = 0xF81F;
/// Yellow color: R=31, G=63, B=0
pub const YELLOW: u16 = 0xFFE0;
/// Cyan color: R=0, G=63, B=31
pub const CYAN: u16 = 0x07FF;

// Less commonly used colors
/// Brown color
pub const BROWN: u16 = 0xBC40;
/// Brownish red color
pub const BRRED: u16 = 0xFC07;
/// Gray color
pub const GRAY: u16 = 0x8430;
/// Dark blue color
pub const DARKBLUE: u16 = 0x01CF;
/// Light blue color
pub const LIGHTBLUE: u16 = 0x7D7C;
/// Gray-blue color
pub const GRAYBLUE: u16 = 0x5458;
/// Light green color
pub const LIGHTGREEN: u16 = 0x841F;
/// Light gray color (window background color)
pub const LGRAY: u16 = 0xC618;
/// Light gray-blue color (middle layer color)
pub const LGRAYBLUE: u16 = 0xA651;
/// Light brown-blue color (selection item inverse color)
pub const LBBLUE: u16 = 0x2B12;

/// List of all predefined colors for cycling through
pub const PREDEFINED_COLORS: &[u16] = &[
    WHITE, BLACK, BLUE, RED, MAGENTA, GREEN, CYAN, YELLOW, BRRED, GRAY, LGRAY, BROWN,
];

/// RGB LCD device structure
#[derive(Debug, Clone)]
pub struct RgbLcdDev {
    /// Panel width (fixed parameter, does not change with display orientation)
    pub pwidth: u32,
    /// Panel height (fixed parameter, does not change with display orientation)
    pub pheight: u32,
    /// Horizontal sync width
    pub hsw: u16,
    /// Vertical sync width
    pub vsw: u16,
    /// Horizontal back porch
    pub hbp: u16,
    /// Vertical back porch
    pub vbp: u16,
    /// Horizontal front porch
    pub hfp: u16,
    /// Vertical front porch
    pub vfp: u16,
    /// Active layer number: 0/1
    pub activelayer: u8,
    /// Display orientation: 0=portrait, 1=landscape
    pub dir: u8,
    /// RGB LCD ID
    pub id: u16,
    /// Pixel clock frequency in Hz
    pub pclk_hz: u32,
    /// Display width (changes with orientation)
    pub width: u16,
    /// Display height (changes with orientation)
    pub height: u16,
}

impl RgbLcdDev {
    /// Create a new RGB LCD device with the given parameters
    pub fn new(pwidth: u32, pheight: u32, id: u16) -> Self {
        let mut dev = Self {
            pwidth,
            pheight,
            hsw: 0,
            vsw: 0,
            hbp: 0,
            vbp: 0,
            hfp: 0,
            vfp: 0,
            activelayer: 0,
            dir: 0,
            id,
            pclk_hz: 0,
            width: pwidth as u16,
            height: pheight as u16,
        };

        // Set initial dimensions based on portrait orientation
        dev.set_orientation(0);
        dev
    }

    /// Set display orientation
    pub fn set_orientation(&mut self, dir: u8) {
        self.dir = dir;

        match self.dir {
            0 => {
                // Portrait mode
                self.width = self.pheight as u16;
                self.height = self.pwidth as u16;
            }
            1 => {
                // Landscape mode
                self.width = self.pwidth as u16;
                self.height = self.pheight as u16;
            }
            _ => {
                // Default to portrait mode
                self.width = self.pheight as u16;
                self.height = self.pwidth as u16;
            }
        }
    }
}

/// Supported RGB LCD screens database
#[derive(Debug, Clone)]
pub struct RgbLcdScreen {
    /// Screen ID
    pub id: u16,
    /// Screen model name
    pub name: &'static str,
    /// Panel width
    pub width: u16,
    /// Panel height
    pub height: u16,
    /// Pixel clock frequency in Hz
    pub pclk_hz: u32,
    /// Horizontal sync width
    pub hsw: u16,
    /// Horizontal back porch
    pub hbp: u16,
    /// Horizontal front porch
    pub hfp: u16,
    /// Vertical sync width
    pub vsw: u16,
    /// Vertical back porch
    pub vbp: u16,
    /// Vertical front porch
    pub vfp: u16,
}

/// List of supported screens with timing parameters
pub const SUPPORTED_SCREENS: &[RgbLcdScreen] = &[
    RgbLcdScreen {
        id: 0x4342,
        name: "4.3 inch 480x272 RGB screen",
        width: 480,
        height: 272,
        pclk_hz: 9_000_000, // 9MHz
        hsw: 4,
        hbp: 43,
        hfp: 8,
        vsw: 4,
        vbp: 12,
        vfp: 8,
    },
    RgbLcdScreen {
        id: 0x4384,
        name: "7 inch 800x480 RGB screen",
        width: 800,
        height: 480,
        pclk_hz: 20_000_000, // 20MHz
        hsw: 48,
        hbp: 88,
        hfp: 40,
        vsw: 3,
        vbp: 32,
        vfp: 13,
    },
    RgbLcdScreen {
        id: 0x7084,
        name: "ATK-MD0700R-800480",
        width: 800,
        height: 480,
        pclk_hz: 20_000_000, // 20MHz
        hsw: 1,
        hbp: 46,
        hfp: 210,
        vsw: 1,
        vbp: 23,
        vfp: 22,
    },
];

/// Find a screen by its ID
pub fn find_screen_by_id(id: u16) -> Option<&'static RgbLcdScreen> {
    SUPPORTED_SCREENS.iter().find(|screen| screen.id == id)
}

/// Configure timing parameters based on screen ID
pub fn config_timing(dev: &mut RgbLcdDev, screen_id: u16) {
    if let Some(screen) = find_screen_by_id(screen_id) {
        dev.pwidth = screen.width as u32;
        dev.pheight = screen.height as u32;
        dev.hsw = screen.hsw;
        dev.hbp = screen.hbp;
        dev.hfp = screen.hfp;
        dev.vsw = screen.vsw;
        dev.vbp = screen.vbp;
        dev.vfp = screen.vfp;
        dev.pclk_hz = screen.pclk_hz;
        dev.id = screen_id;

        // Update display dimensions based on current orientation
        dev.set_orientation(dev.dir);
    }
}

/// RGB LCD driver
pub struct RgbLcd<'d> {
    /// LCD CAM peripheral
    _lcd_cam: LcdCam<'d, esp_hal::Blocking>,
    /// RGB LCD device parameters
    pub dev: RgbLcdDev,
    /// Phantom data to tie lifetime to the peripheral
    _phantom: PhantomData<&'d ()>,
}

impl<'d> RgbLcd<'d> {
    /// Create a new RGB LCD driver
    pub fn new(
        lcd_cam: impl esp_hal::peripheral::Peripheral<P = esp_hal::peripherals::LCD_CAM> + 'd,
        mut dev: RgbLcdDev,
    ) -> Self {
        // Configure timing based on screen ID
        config_timing(&mut dev, dev.id);

        Self {
            _lcd_cam: LcdCam::new(lcd_cam),
            dev,
            _phantom: PhantomData,
        }
    }

    /// Initialize the RGB LCD with default configuration
    pub fn init<De, Hsync, Vsync, Pclk>(
        &mut self,
        _de: De,
        _hsync: Hsync,
        _vsync: Vsync,
        _pclk: Pclk,
        _rgb_pins: [Option<Box<dyn OutputPin + 'd>>; 16],
    ) -> Result<(), LcdError>
    where
        De: OutputPin + 'd,
        Hsync: OutputPin + 'd,
        Vsync: OutputPin + 'd,
        Pclk: OutputPin + 'd,
    {
        // In a real implementation, this would initialize the LCD with the provided pins
        // For now, we'll just return Ok(())
        Ok(())
    }

    /// Set display orientation
    pub fn set_orientation(&mut self, dir: u8) {
        self.dev.set_orientation(dir);
    }

    /// Get display width
    pub fn width(&self) -> u16 {
        self.dev.width
    }

    /// Get display height
    pub fn height(&self) -> u16 {
        self.dev.height
    }

    /// Update timing configuration for a specific screen ID
    pub fn update_timing(&mut self, screen_id: u16) {
        config_timing(&mut self.dev, screen_id);
    }

    /// Calculate bounce buffer size based on screen ID
    pub fn bounce_buffer_size(&self) -> usize {
        match self.dev.id {
            0x4384 | 0x7084 => 800 * 20,
            0x4342 => 480 * 20,
            _ => 480 * 20, // Default fallback
        }
    }

    /// Clear the screen with a specific color
    pub fn clear(&mut self, _color: u16) -> Result<(), LcdError> {
        // Placeholder implementation
        // In a real implementation, this would fill the entire screen with the specified color
        Ok(())
    }

    /// Show a string at a specific position
    pub fn show_string(
        &mut self,
        _x: u16,
        _y: u16,
        _width: u16,
        _height: u16,
        _font_size: u16,
        _text: &str,
        _color: u16,
    ) -> Result<(), LcdError> {
        // Placeholder implementation
        // In a real implementation, this would render text at the specified position
        Ok(())
    }
}

/// Example task demonstrating RGB LCD functionality
/// This function cycles through different background colors and displays test strings
pub async fn example_task(mut display: RgbLcd<'_>) {
    let test_strings = [
        ("ESP32-S3", RED),
        ("RGBLCD TEST", RED),
        ("ATOM@ALIENTEK", RED),
    ];

    let mut color_index = 0;

    loop {
        // Cycle through predefined colors
        let color = PREDEFINED_COLORS[color_index];
        let _ = display.clear(color);

        // Display test strings
        let _ = display.show_string(10, 40, 240, 32, 32, test_strings[0].0, test_strings[0].1);
        let _ = display.show_string(10, 80, 240, 24, 24, test_strings[1].0, test_strings[1].1);
        let _ = display.show_string(10, 110, 240, 16, 16, test_strings[2].0, test_strings[2].1);

        color_index = (color_index + 1) % PREDEFINED_COLORS.len();

        // Simulate LED toggle and delay
        // In a real implementation, you would control an actual LED here
        embassy_time::Timer::after_millis(1000).await;
    }
}