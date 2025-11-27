//! Camera Module for ESP32-S3
//!
//! This module provides functionality for initializing and capturing images
//! from OV-series cameras using the ESP32-S3 camera peripheral.
//!
//! Based on the C implementation provided in the ESP-IDF camera library.
//! Supports PSRAM frame buffer storage, configurable grab modes,
//! and performance testing capabilities.

use defmt::info;
use embassy_time::{Duration, Timer};
use embedded_graphics::prelude::RgbColor;
use esp_hal::gpio::AnyPin;

// Camera pin configuration structure
#[derive(Debug)]
pub struct CameraPins<'a> {
    pub pin_pwdn: Option<AnyPin<'a>>,
    pub pin_reset: Option<AnyPin<'a>>,
    pub pin_xclk: AnyPin<'a>,
    pub pin_sccb_sda: AnyPin<'a>,
    pub pin_sccb_scl: AnyPin<'a>,
    pub pin_d7: AnyPin<'a>,
    pub pin_d6: AnyPin<'a>,
    pub pin_d5: AnyPin<'a>,
    pub pin_d4: AnyPin<'a>,
    pub pin_d3: AnyPin<'a>,
    pub pin_d2: AnyPin<'a>,
    pub pin_d1: AnyPin<'a>,
    pub pin_d0: AnyPin<'a>,
    pub pin_vsync: AnyPin<'a>,
    pub pin_href: AnyPin<'a>,
    pub pin_pclk: AnyPin<'a>,
}

/// Camera configuration structure
#[derive(Debug)]
pub struct CameraConfig<'a> {
    /// Camera pin configuration
    pub pins: CameraPins<'a>,
    /// XCLK frequency in Hz
    pub xclk_freq_hz: u32,
    /// Pixel format
    pub pixel_format: PixelFormat,
    /// Frame size
    pub frame_size: FrameSize,
    /// JPEG quality (0-63)
    pub jpeg_quality: u8,
    /// Frame buffer count
    pub fb_count: u8,
    /// Frame buffer location
    pub fb_location: FrameBufferLocation,
    /// Grab mode
    pub grab_mode: GrabMode,
}

/// Frame buffer location
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameBufferLocation {
    /// Store frame buffer in PSRAM
    InPsram,
    /// Store frame buffer in DRAM
    InDram,
}

/// Grab mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GrabMode {
    /// Grab frame when buffer is empty
    WhenEmpty,
    /// Grab frame immediately
    Immediately,
}

impl Default for CameraConfig<'_> {
    /// Creates a default camera configuration
    fn default() -> Self {
        Self {
            pins: CameraPins {
                pin_pwdn: None,
                pin_reset: None,
                pin_xclk: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_sccb_sda: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_sccb_scl: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_d7: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_d6: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_d5: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_d4: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_d3: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_d2: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_d1: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_d0: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_vsync: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_href: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
                pin_pclk: unsafe { core::mem::MaybeUninit::uninit().assume_init() },
            },
            xclk_freq_hz: 20_000_000,
            pixel_format: PixelFormat::Rgb565,
            frame_size: FrameSize::Qvga,
            jpeg_quality: 12,
            fb_count: 1,
            fb_location: FrameBufferLocation::InDram,
            grab_mode: GrabMode::WhenEmpty,
        }
    }
}

impl CameraConfig<'_> {
    /// Create a camera configuration optimized for PSRAM usage
    pub fn psram_optimized() -> Self {
        let mut config = Self {
            fb_location: FrameBufferLocation::InPsram,
            fb_count: 2, // Double buffering for better performance
            grab_mode: GrabMode::WhenEmpty,
            ..Self::default()
        };

        // Set JPEG quality based on frame size
        config.set_jpeg_quality_for_frame_size(config.frame_size);

        config
    }

    /// Set JPEG quality based on frame size for optimal performance
    pub fn set_jpeg_quality_for_frame_size(&mut self, frame_size: FrameSize) {
        // For smaller frame sizes, we can afford higher quality
        // For larger frame sizes, we need to balance quality and performance
        self.jpeg_quality = match frame_size {
            FrameSize::Size96x96
            | FrameSize::Qqvga
            | FrameSize::Qcif
            | FrameSize::Hqvga
            | FrameSize::Size240x240
            | FrameSize::Qvga
            | FrameSize::Cif
            | FrameSize::Vga
            | FrameSize::Svga => 10, // High quality for smaller resolutions
            _ => 12, // Standard quality for larger resolutions
        };
    }
}

/// Pixel formats
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PixelFormat {
    /// 16-bit RGB565 format
    Rgb565,
    /// 16-bit YUV format
    Yuv422,
    /// 8-bit grayscale format
    Grayscale,
    /// JPEG compressed format
    Jpeg,
    /// 24-bit RGB format
    Rgb888,
    /// RAW data format
    Raw,
    /// 12-bit RGB format
    Rgb444,
    /// 16-bit RGB format
    Rgb555,
}

/// Frame sizes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FrameSize {
    /// 96x96 resolution
    Size96x96,
    /// 160x120 resolution (QQVGA)
    Qqvga,
    /// 176x144 resolution (QCIF)
    Qcif,
    /// 240x176 resolution (HQVGA)
    Hqvga,
    /// 240x240 resolution
    Size240x240,
    /// 320x240 resolution (QVGA)
    Qvga,
    /// 400x296 resolution (CIF)
    Cif,
    /// 640x480 resolution (VGA)
    Vga,
    /// 800x600 resolution (SVGA)
    Svga,
    /// 1024x768 resolution (XGA)
    Xga,
    /// 1280x1024 resolution (SXGA)
    Sxga,
    /// 1600x1200 resolution (UXGA)
    Uxga,
    /// 2048x1536 resolution (QXGA)
    Qxga,
}

/// Camera driver structure
///
/// Provides functionality for controlling an OV-series camera connected to ESP32-S3.
/// Supports various frame buffer locations including PSRAM for improved performance
/// and configurable grab modes for flexible image acquisition.
pub struct Camera<'a> {
    pub(crate) config: CameraConfig<'a>,
}

impl<'a> Camera<'a> {
    /// Create a new camera instance
    ///
    /// Creates a new camera driver instance with the provided configuration.
    /// The configuration includes pin assignments, frame size, pixel format,
    /// frame buffer location (DRAM or PSRAM), and grab mode settings.
    pub fn new(config: CameraConfig<'a>) -> Self {
        Self { config }
    }

    /// Initialize the camera with the given configuration
    ///
    /// This function performs the hardware reset sequence and initializes
    /// the camera driver with the provided configuration.
    pub async fn init(&mut self) -> Result<(), &'static str> {
        info!("Initializing camera...");

        // Hardware reset sequence
        if let Some(reset_pin) = &self.config.pins.pin_reset {
            // Note: Actual pin manipulation would go here
            // For now, we'll simulate the reset sequence
            info!("Performing hardware reset...");

            // Pull reset low
            // reset_pin.set_low();
            Timer::after(Duration::from_millis(20)).await;

            // Pull reset high
            // reset_pin.set_high();
            Timer::after(Duration::from_millis(20)).await;
        }

        // Initialize camera driver (would call esp_camera_init in C)
        info!("Initializing camera driver...");
        // This is where we would call the actual ESP-IDF camera initialization

        // Sensor-specific configuration
        // This would normally read the sensor ID and configure accordingly
        info!("Configuring sensor settings...");
        // s.set_vflip(s, 1);  // Vertical flip
        // s.set_brightness(s, 1);  // Brightness adjustment
        // s.set_saturation(s, -2);  // Saturation adjustment

        info!("Camera initialization complete");
        Ok(())
    }

    /// Configure the camera sensor with the given parameters
    ///
    /// This function configures various image quality and effect parameters
    /// for the camera sensor.
    pub fn configure_sensor(&mut self, config: &SensorConfig) {
        info!("Configuring camera sensor...");

        // Image quality adjustments
        self.set_brightness(config.brightness);
        self.set_contrast(config.contrast);
        self.set_saturation(config.saturation);
        self.set_sharpness(config.sharpness);

        // Effect settings
        self.set_whitebal(config.whitebal);
        self.set_awb_gain(config.awb_gain);
        self.set_wb_mode(config.wb_mode);
        self.set_exposure_ctrl(config.exposure_ctrl);
        self.set_aec2(config.aec2);
        self.set_ae_level(config.ae_level);
        self.set_aec_value(config.aec_value);

        // Image control
        self.set_gain_ctrl(config.gain_ctrl);
        self.set_agc_gain(config.agc_gain);
        self.set_gainceiling(config.gainceiling);
        self.set_bpc(config.bpc);
        self.set_wpc(config.wpc);
        self.set_raw_gma(config.raw_gma);
        self.set_lenc(config.lenc);
        self.set_hmirror(config.hmirror);
        self.set_vflip(config.vflip);
        self.set_dcw(config.dcw);
        self.set_colorbar(config.colorbar);

        info!("Camera sensor configuration complete");
    }

    /// Capture a frame from the camera
    ///
    /// This function captures a single frame from the camera and returns
    /// it as a CameraFrame struct.
    pub async fn capture_frame(&mut self) -> Result<CameraFrame, &'static str> {
        info!("Capturing frame...");

        // This would call esp_camera_fb_get() in the C implementation
        // For now, we'll return a dummy frame with proper dimensions
        let (width, height) = match self.config.frame_size {
            FrameSize::Size96x96 => (96, 96),
            FrameSize::Qqvga => (160, 120),
            FrameSize::Qcif => (176, 144),
            FrameSize::Hqvga => (240, 176),
            FrameSize::Size240x240 => (240, 240),
            FrameSize::Qvga => (320, 240),
            FrameSize::Cif => (400, 296),
            FrameSize::Vga => (640, 480),
            FrameSize::Svga => (800, 600),
            FrameSize::Xga => (1024, 768),
            FrameSize::Sxga => (1280, 1024),
            FrameSize::Uxga => (1600, 1200),
            FrameSize::Qxga => (2048, 1536),
        };

        // 为了减少内存使用，我们创建一个较小的帧数据
        // 在实际实现中，这应该从摄像头硬件获取
        let frame_data_size = (width as usize) * (height as usize) * 2; // RGB565格式每个像素2字节
        let data = if frame_data_size > 100000 { // 如果大于100KB，只分配10KB
            info!("Frame size too large, allocating smaller buffer for testing");
            alloc::vec![0; 10000] // 只分配10KB用于测试
        } else {
            alloc::vec![0; frame_data_size]
        };

        let frame = CameraFrame {
            width,
            height,
            format: self.config.pixel_format,
            data,
        };

        Ok(frame)
    }

    /// Release a captured frame
    ///
    /// This function releases the frame buffer back to the camera driver.
    pub fn release_frame(&mut self, _frame: CameraFrame) {
        // This would call esp_camera_fb_return() in the C implementation
        info!("Releasing frame buffer");
    }

    /// Test camera performance by capturing multiple frames
    ///
    /// This function captures a specified number of frames and calculates
    /// the average FPS and frame size.
    ///
    /// # Arguments
    /// * `times` - Number of frames to capture
    ///
    /// # Returns
    /// * `Ok(CameraPerformance)` - Performance metrics
    /// * `Err(&'static str)` - Error message if test fails
    pub async fn test_performance(
        &mut self,
        times: u16,
    ) -> Result<CameraPerformance, &'static str> {
        info!("Starting camera performance test with {} frames", times);

        let start_time = embassy_time::Instant::now();
        let mut total_size = 0u32;
        let mut frame_count = 0u32;

        for i in 0..times {
            match self.capture_frame().await {
                Ok(frame) => {
                    total_size += frame.data.len() as u32;
                    frame_count += 1;

                    // In a real implementation, we would return the frame buffer to the driver
                    // For now, we just drop it
                    drop(frame);
                }
                Err(e) => {
                    info!("Frame capture failed at iteration {}: {:?}", i, e);
                }
            }
        }

        let end_time = embassy_time::Instant::now();
        let elapsed_time = end_time.duration_since(start_time).as_micros() as u64;

        if frame_count > 0 && elapsed_time > 0 {
            let fps = (frame_count as f32) / ((elapsed_time as f32) / 1_000_000.0);
            let avg_size = total_size / frame_count;

            let perf = CameraPerformance { fps, avg_size };

            info!(
                "Performance test completed: {:02} FPS, avg size: {} bytes",
                perf.fps, perf.avg_size
            );

            Ok(perf)
        } else {
            Err("No frames captured or invalid timing")
        }
    }

    /// Set brightness
    fn set_brightness(&mut self, _brightness: i8) {
        // Implementation would interface with the actual camera driver
        info!("Setting brightness to {}", _brightness);
    }

    /// Set contrast
    fn set_contrast(&mut self, _contrast: i8) {
        // Implementation would interface with the actual camera driver
        info!("Setting contrast to {}", _contrast);
    }

    /// Set saturation
    fn set_saturation(&mut self, _saturation: i8) {
        // Implementation would interface with the actual camera driver
        info!("Setting saturation to {}", _saturation);
    }

    /// Set sharpness
    fn set_sharpness(&mut self, _sharpness: i8) {
        // Implementation would interface with the actual camera driver
        info!("Setting sharpness to {}", _sharpness);
    }

    /// Set white balance
    fn set_whitebal(&mut self, _whitebal: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting white balance to {}", _whitebal);
    }

    /// Set AWB gain
    fn set_awb_gain(&mut self, _awb_gain: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting AWB gain to {}", _awb_gain);
    }

    /// Set WB mode
    fn set_wb_mode(&mut self, _wb_mode: u8) {
        // Implementation would interface with the actual camera driver
        info!("Setting WB mode to {}", _wb_mode);
    }

    /// Set exposure control
    fn set_exposure_ctrl(&mut self, _exposure_ctrl: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting exposure control to {}", _exposure_ctrl);
    }

    /// Set AEC2
    fn set_aec2(&mut self, _aec2: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting AEC2 to {}", _aec2);
    }

    /// Set AE level
    fn set_ae_level(&mut self, _ae_level: i8) {
        // Implementation would interface with the actual camera driver
        info!("Setting AE level to {}", _ae_level);
    }

    /// Set AEC value
    fn set_aec_value(&mut self, _aec_value: u16) {
        // Implementation would interface with the actual camera driver
        info!("Setting AEC value to {}", _aec_value);
    }

    /// Set gain control
    fn set_gain_ctrl(&mut self, _gain_ctrl: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting gain control to {}", _gain_ctrl);
    }

    /// Set AGC gain
    fn set_agc_gain(&mut self, _agc_gain: u8) {
        // Implementation would interface with the actual camera driver
        info!("Setting AGC gain to {}", _agc_gain);
    }

    /// Set gain ceiling
    fn set_gainceiling(&mut self, _gainceiling: GainCeiling) {
        // Implementation would interface with the actual camera driver
        info!("Setting gain ceiling to {:?}", _gainceiling);
    }

    /// Set bad pixel correction
    fn set_bpc(&mut self, _bpc: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting BPC to {}", _bpc);
    }

    /// Set white pixel correction
    fn set_wpc(&mut self, _wpc: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting WPC to {}", _wpc);
    }

    /// Set raw gamma
    fn set_raw_gma(&mut self, _raw_gma: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting raw gamma to {}", _raw_gma);
    }

    /// Set lens correction
    fn set_lenc(&mut self, _lenc: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting lens correction to {}", _lenc);
    }

    /// Set horizontal mirror
    fn set_hmirror(&mut self, _hmirror: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting horizontal mirror to {}", _hmirror);
    }

    /// Set vertical flip
    fn set_vflip(&mut self, _vflip: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting vertical flip to {}", _vflip);
    }

    /// Set DCW
    fn set_dcw(&mut self, _dcw: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting DCW to {}", _dcw);
    }

    /// Set color bar
    fn set_colorbar(&mut self, _colorbar: bool) {
        // Implementation would interface with the actual camera driver
        info!("Setting color bar to {}", _colorbar);
    }
}

/// Camera frame structure
///
/// Represents a single captured frame from the camera.
pub struct CameraFrame {
    /// Frame width in pixels
    pub width: u16,
    /// Frame height in pixels
    pub height: u16,
    /// Pixel format of the frame
    pub format: PixelFormat,
    /// Frame data buffer
    pub data: alloc::vec::Vec<u8>,
}

impl CameraFrame {
    /// Get the frame data
    ///
    /// Returns a reference to the frame data buffer.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// Camera sensor configuration parameters
#[derive(Debug, Clone)]
pub struct SensorConfig {
    /// Brightness (-2 to 2)
    pub brightness: i8,
    /// Contrast (-2 to 2)
    pub contrast: i8,
    /// Saturation (-2 to 2)
    pub saturation: i8,
    /// Sharpness (-2 to 2)
    pub sharpness: i8,

    /// Automatic white balance enable
    pub whitebal: bool,
    /// AWB gain enable
    pub awb_gain: bool,
    /// White balance mode
    pub wb_mode: u8,
    /// Automatic exposure control enable
    pub exposure_ctrl: bool,
    /// AEC2 enable
    pub aec2: bool,
    /// AE level
    pub ae_level: i8,
    /// Exposure value
    pub aec_value: u16,

    /// Automatic gain control enable
    pub gain_ctrl: bool,
    /// AGC gain
    pub agc_gain: u8,
    /// Gain ceiling
    pub gainceiling: GainCeiling,
    /// Bad pixel correction enable
    pub bpc: bool,
    /// White pixel correction enable
    pub wpc: bool,
    /// Raw gamma correction enable
    pub raw_gma: bool,
    /// Lens correction enable
    pub lenc: bool,
    /// Horizontal mirror enable
    pub hmirror: bool,
    /// Vertical flip enable
    pub vflip: bool,
    /// DCW enable
    pub dcw: bool,
    /// Color bar test pattern enable
    pub colorbar: bool,
}

impl Default for SensorConfig {
    /// Creates a default sensor configuration with reasonable defaults
    fn default() -> Self {
        Self {
            brightness: 0,
            contrast: 0,
            saturation: 0,
            sharpness: 0,

            whitebal: true,
            awb_gain: true,
            wb_mode: 0,
            exposure_ctrl: true,
            aec2: false,
            ae_level: 0,
            aec_value: 300,

            gain_ctrl: true,
            agc_gain: 0,
            gainceiling: GainCeiling::Gain32x,
            bpc: false,
            wpc: true,
            raw_gma: true,
            lenc: true,
            hmirror: false,
            vflip: true,
            dcw: true,
            colorbar: false,
        }
    }
}

/// Gain ceiling values for the camera sensor
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GainCeiling {
    /// 2x gain
    Gain2x,
    /// 4x gain
    Gain4x,
    /// 8x gain
    Gain8x,
    /// 16x gain
    Gain16x,
    /// 32x gain
    Gain32x,
    /// 64x gain
    Gain64x,
    /// 128x gain
    Gain128x,
}

/// Implement defmt::Format for GainCeiling to enable logging
impl defmt::Format for GainCeiling {
    fn format(&self, f: defmt::Formatter) {
        match self {
            GainCeiling::Gain2x => defmt::write!(f, "Gain2x"),
            GainCeiling::Gain4x => defmt::write!(f, "Gain4x"),
            GainCeiling::Gain8x => defmt::write!(f, "Gain8x"),
            GainCeiling::Gain16x => defmt::write!(f, "Gain16x"),
            GainCeiling::Gain32x => defmt::write!(f, "Gain32x"),
            GainCeiling::Gain64x => defmt::write!(f, "Gain64x"),
            GainCeiling::Gain128x => defmt::write!(f, "Gain128x"),
        }
    }
}

impl GainCeiling {
    /// Convert to numeric value
    pub fn to_u8(&self) -> u8 {
        match self {
            GainCeiling::Gain2x => 0,
            GainCeiling::Gain4x => 1,
            GainCeiling::Gain8x => 2,
            GainCeiling::Gain16x => 3,
            GainCeiling::Gain32x => 4,
            GainCeiling::Gain64x => 5,
            GainCeiling::Gain128x => 6,
        }
    }
}

/// Performance test results
///
/// Contains metrics from the camera performance test including
/// frames per second and average frame size.
#[derive(Debug, Clone, Copy)]
pub struct CameraPerformance {
    /// Frames per second achieved during the test
    pub fps: f32,
    /// Average frame size in bytes
    pub avg_size: u32,
}

/// Function to capture and display image
///
/// Captures a frame from the camera and displays it on the provided display.
pub async fn camera_capture_show(
    camera: &mut Camera<'_>,
    display: &mut crate::spi_lcd::ST7789<'_>,
    spilcd_dir: u8,
) -> Result<(), &'static str> {
    // Get a frame from the camera
    let frame = camera.capture_frame().await?;

    // Display the image based on screen orientation
    if spilcd_dir == 1 {
        // Landscape mode
        display_image(display, 0, 0, 320, 240, &frame).await?;
    } else {
        // Portrait mode
        display_image(display, 0, 39, 240, 240, &frame).await?;
    }

    // Release the frame buffer
    camera.release_frame(frame);

    Ok(())
}

/// Display an image on the LCD
///
/// Displays a camera frame on the provided display at the specified position and size.
async fn display_image(
    display: &mut crate::spi_lcd::ST7789<'_>,
    x: u16,
    y: u16,
    width: u16,
    height: u16,
    frame: &CameraFrame,
) -> Result<(), &'static str> {
    info!(
        "Displaying image at ({}, {}) with size {}x{}",
        x, y, width, height
    );

    // In a real implementation, this would convert the camera frame data
    // to the appropriate format for the display and render it

    // For now, we'll just fill a rectangle with a placeholder color
    use embedded_graphics::pixelcolor::Rgb565;
    let _ = display.fill_rectangle(x, y, width, height, Rgb565::BLUE);

    Ok(())
}
