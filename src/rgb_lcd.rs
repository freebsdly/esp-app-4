use core::marker::PhantomData;
use esp_hal::{
    dma::{DmaChannel, DmaTxBuf, TxChannelFor},
    gpio::OutputPin,
    lcd_cam::{
        lcd::dpi::{Config, Dpi, Format, FrameTiming},
        LcdCam,
    },
    peripherals::LCD_CAM,
    time::Rate,
    Blocking,
};

use crate::backlight::{BacklightManager, Xl9555Backlight};

/// Display configuration similar to STM32's DisplayConfig
#[derive(Debug, Clone, Copy)]
pub struct DisplayConfig {
    /// Active width in pixels
    pub active_width: u32,
    /// Active height in pixels
    pub active_height: u32,
    /// Horizontal back porch in pixels
    pub h_back_porch: u16,
    /// Horizontal front porch in pixels
    pub h_front_porch: u16,
    /// Horizontal sync pulse width in pixels
    pub h_sync: u16,
    /// Vertical back porch in lines
    pub v_back_porch: u16,
    /// Vertical front porch in lines
    pub v_front_porch: u16,
    /// Vertical sync pulse width in lines
    pub v_sync: u16,
    /// Target frame rate in Hz
    pub frame_rate: u32,
    /// Horizontal sync polarity (true = active high)
    pub h_sync_pol: bool,
    /// Vertical sync polarity (true = active high)
    pub v_sync_pol: bool,
    /// Data enable polarity (true = active high)
    pub no_data_enable_pol: bool,
    /// Pixel clock polarity (true = active high)
    pub pixel_clock_pol: bool,
}

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
    RgbLcdScreen {
        id: 0x7085,
        name: "ATK-MD700R2-800480",
        width: 800,
        height: 480,
        pclk_hz: 33_300_000, // 33.3MHz
        hsw: 20,
        hbp: 46,
        hfp: 210,
        vsw: 2,
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

impl From<RgbLcdScreen> for DisplayConfig {
    fn from(screen: RgbLcdScreen) -> Self {
        DisplayConfig {
            active_width: screen.width as u32,
            active_height: screen.height as u32,
            h_back_porch: screen.hbp,
            h_front_porch: screen.hfp,
            h_sync: screen.hsw,
            v_back_porch: screen.vbp,
            v_front_porch: screen.vfp,
            v_sync: screen.vsw,
            frame_rate: 60, // 默认帧率为60Hz
            h_sync_pol: false,
            v_sync_pol: false,
            no_data_enable_pol: false,
            pixel_clock_pol: false,
        }
    }
}

/// RGB LCD driver
pub struct RgbLcd<'d> {
    /// DPI interface
    dpi: Option<Dpi<'d, Blocking>>,
    /// RGB LCD device parameters
    pub dev: RgbLcdDev,
    /// Backlight manager
    backlight: BacklightManager<Xl9555Backlight>,
    /// Phantom data to tie lifetime to the peripheral
    _phantom: PhantomData<&'d ()>,
}

impl<'d> RgbLcd<'d> {
    /// Create a new RGB LCD driver
    pub fn new<CH: DmaChannel + TxChannelFor<LCD_CAM<'d>>>(
        lcd_cam: esp_hal::peripherals::LCD_CAM<'d>,
        channel: CH,
        mut dev: RgbLcdDev,
        de: impl OutputPin + 'd,
        hsync: impl OutputPin + 'd,
        vsync: impl OutputPin + 'd,
        pclk: impl OutputPin + 'd,
        mut data_pins: [Option<impl OutputPin + 'd>; 16],
    ) -> Self {
        // Configure timing based on screen ID
        let screen_id = dev.id;
        config_timing(&mut dev, screen_id);

        let lcd_cam = LcdCam::new(lcd_cam);

        let config = Config::default()
            .with_frequency(Rate::from_hz(dev.pclk_hz))
            .with_format(Format {
                enable_2byte_mode: true,
                ..Default::default()
            })
            .with_timing(FrameTiming {
                horizontal_active_width: dev.pwidth as usize,
                horizontal_total_width: (dev.pwidth as usize)
                    + (dev.hbp as usize)
                    + (dev.hfp as usize),
                horizontal_blank_front_porch: dev.hfp as usize,

                vertical_active_height: dev.pheight as usize,
                vertical_total_height: (dev.pheight as usize)
                    + (dev.vbp as usize)
                    + (dev.vfp as usize),
                vertical_blank_front_porch: dev.vfp as usize,

                hsync_width: dev.hsw as usize,
                vsync_width: dev.vsw as usize,

                hsync_position: 0,
            });

        let mut dpi = Dpi::new(lcd_cam.lcd, channel, config)
            .expect("Failed to create DPI interface")
            .with_de(de)
            .with_hsync(hsync)
            .with_vsync(vsync)
            .with_pclk(pclk);

        // Attach data pins
        macro_rules! attach_data_pin {
            ($index:expr, $method:ident) => {
                if let Some(pin) = data_pins[$index].take() {
                    dpi = dpi.$method(pin);
                }
            };
        }

        attach_data_pin!(0, with_data0);
        attach_data_pin!(1, with_data1);
        attach_data_pin!(2, with_data2);
        attach_data_pin!(3, with_data3);
        attach_data_pin!(4, with_data4);
        attach_data_pin!(5, with_data5);
        attach_data_pin!(6, with_data6);
        attach_data_pin!(7, with_data7);
        attach_data_pin!(8, with_data8);
        attach_data_pin!(9, with_data9);
        attach_data_pin!(10, with_data10);
        attach_data_pin!(11, with_data11);
        attach_data_pin!(12, with_data12);
        attach_data_pin!(13, with_data13);
        attach_data_pin!(14, with_data14);
        attach_data_pin!(15, with_data15);

        // Create backlight manager with XL9555 controller
        let xl9555_backlight = Xl9555Backlight::new();
        let backlight = BacklightManager::new(xl9555_backlight);

        Self {
            dpi: Some(dpi),
            dev,
            backlight,
            _phantom: PhantomData,
        }
    }

    /// Send a frame to the LCD display
    pub fn send_frame(&mut self, buffer: DmaTxBuf) -> Result<(), esp_hal::dma::DmaError> {
        let dpi = self.dpi.take().unwrap();
        let transfer = dpi.send(true, buffer);
        match transfer {
            Ok(t) => {
                let (result, dpi, _buf) = t.wait();
                self.dpi = Some(dpi);
                result.map_err(|e| e)
            }
            Err(e) => {
                self.dpi = Some(e.1);
                Err(e.0)
            }
        }
    }

    /// Send a frame to the LCD display and wait for completion
    pub fn send_frame_blocking(&mut self, buffer: DmaTxBuf) -> Result<(), esp_hal::dma::DmaError> {
        let dpi = self.dpi.take().unwrap();
        let transfer = dpi.send(true, buffer);
        match transfer {
            Ok(t) => {
                let (result, dpi, _buf) = t.wait();
                self.dpi = Some(dpi);
                result.map_err(|e| e)
            }
            Err(e) => {
                self.dpi = Some(e.1);
                Err(e.0)
            }
        }
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
            0x4384 | 0x7084 | 0x7085 => 800 * 20,
            0x4342 => 480 * 20,
            _ => 480 * 20, // Default fallback
        }
    }

    /// Set backlight brightness
    pub async fn set_backlight(
        &mut self,
        brightness: u8,
    ) -> Result<(), esp_hal::i2c::master::Error> {
        self.backlight.set_backlight(brightness).await
    }

    /// Turn on backlight
    pub async fn backlight_on(&mut self) -> Result<(), esp_hal::i2c::master::Error> {
        self.backlight.set_backlight(100).await
    }

    /// Turn off backlight
    pub async fn backlight_off(&mut self) -> Result<(), esp_hal::i2c::master::Error> {
        self.backlight.set_backlight(0).await
    }

    /// Draw framebuffer data to the LCD display
    /// 
    /// This function takes framebuffer data and displays it on the LCD.
    /// The data should be in RGB565 format.
    /// 
    /// # Arguments
    /// * `data` - Slice of RGB565 pixel data to display
    /// 
    /// # Returns
    /// * `Ok(())` - Successfully displayed the framebuffer
    /// * `Err(...)` - Failed to display the framebuffer
    pub async fn draw_framebuffer(&mut self, data: &[u8]) -> Result<(), esp_hal::dma::DmaError> {
        // Calculate required buffer size (2 bytes per pixel for RGB565)
        let required_size = (self.dev.width as usize) * (self.dev.height as usize) * 2;
        
        // Ensure we have enough data
        if data.len() < required_size {
            // If we don't have enough data, we could either return an error or pad with black
            // For now, let's just use what we have and it will display partially
            defmt::warn!("Framebuffer data size ({}) is less than required ({}), displaying partial image", data.len(), required_size);
        }

        // Create a DMA buffer
        let buffer_size = data.len().min(required_size).next_multiple_of(4);
        // 创建静态描述符数组和缓冲区
        let descriptors = unsafe {
            static mut DESCRIPTORS: [esp_hal::dma::DmaDescriptor; 32] = [esp_hal::dma::DmaDescriptor::EMPTY; 32];
            &mut DESCRIPTORS[..]
        };
        
        let buffer = unsafe {
            static mut BUFFER: [u8; 800 * 480 * 2] = [0; 800 * 480 * 2]; // 最大支持800*480屏幕
            &mut BUFFER[..buffer_size]
        };
        
        let mut tx_buf = DmaTxBuf::new(descriptors, buffer).unwrap();
        
        // Copy data to buffer
        let buffer_slice = tx_buf.as_mut_slice();
        let copy_size = buffer_slice.len().min(data.len());
        buffer_slice[..copy_size].copy_from_slice(&data[..copy_size]);
        
        // Fill remaining buffer with black (0x0000 in RGB565)
        if copy_size < buffer_slice.len() {
            buffer_slice[copy_size..].fill(0x00);
        }
        
        // Send frame to display
        self.send_frame(tx_buf)
    }
}

impl RgbLcdDev {
    /// Get horizontal sync pulse width
    pub fn hsync_pulse_width(&self) -> u16 {
        self.hsw
    }

    /// Get horizontal back porch
    pub fn hsync_back_porch(&self) -> u16 {
        self.hbp
    }

    /// Get horizontal front porch
    pub fn hsync_front_porch(&self) -> u16 {
        self.hfp
    }

    /// Get vertical sync pulse width
    pub fn vsync_pulse_width(&self) -> u16 {
        self.vsw
    }

    /// Get vertical back porch
    pub fn vsync_back_porch(&self) -> u16 {
        self.vbp
    }

    /// Get vertical front porch
    pub fn vsync_front_porch(&self) -> u16 {
        self.vfp
    }
}

/// Example task demonstrating RGB LCD functionality
/// This function cycles through different background colors and displays test strings
pub async fn example_task() {
    let mut color_index = 0;

    loop {
        // Cycle through predefined colors
        let _color = PREDEFINED_COLORS[color_index];
        // In a real implementation, this would fill the entire screen with the specified color
        // display.clear(color).await;

        // Display test strings
        // In a real implementation, this would render text at the specified position
        // display.show_string(10, 40, 240, 32, 32, test_strings[0].0, test_strings[0].1).await;
        // display.show_string(10, 80, 240, 24, 24, test_strings[1].0, test_strings[1].1).await;
        // display.show_string(10, 110, 240, 16, 16, test_strings[2].0, test_strings[2].1).await;

        color_index = (color_index + 1) % PREDEFINED_COLORS.len();

        // Simulate LED toggle and delay
        // In a real implementation, you would control an actual LED here
        embassy_time::Timer::after_millis(1000).await;
    }
}

/// Control RGB LCD backlight through XL9555 GPIO expander
///
/// This function controls the RGB LCD backlight by manipulating the XL9555 GPIO expander.
/// The backlight is controlled via P1.3 pin of the XL9555 chip.
///
/// # Arguments
/// * `state` - Backlight state: true to turn on, false to turn off
pub async fn set_backlight(state: bool) -> Result<(), esp_hal::i2c::master::Error> {
    crate::xl9555::set_lcd_backlight(state).await
}
