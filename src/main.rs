//! # 正点原子 ESP32-S3 板载开发板
//!
//!     ESP32-S3: 系统核心芯片，集成Wi-Fi/蓝牙双模通信、RISC-V双核处理器及AI加速器，支持2.4GHz无线通信与边缘计算。
//!     TFT-LCD: 采用SPI接口的2.4英寸TFT显示屏，支持RGB565色彩格式，通过LCD_DC、LCD_RST等引脚控制显示内容。
//!     OV2640: 200万像素摄像头模组，支持MIPI接口视频流输出，通过OV_PCLK、OV_VSYNC等信号实现图像采集。
//!
//!     电源管理模块
//!
//!     5V/3.3V双路稳压电路
//!     USB Type-C接口供电与数据传输
//!     电源状态指示灯
//!
//!     核心处理器模块
//!
//!     ESP32-S3芯片引脚分配
//!     RISC-V双核处理器与AI加速器
//!     内置Flash存储与外扩PSRAM
//!
//!     显示与输入模块
//!
//!     SPI接口TFT-LCD驱动电路
//!     4x4矩阵按键扫描电路
//!     红外遥控接收与发射电路
//!
//!     传感器与扩展模块
//!
//!     温湿度/温度传感器接口
//!     三轴加速度计与ALS/PS传感器
//!     TF卡存储接口与EEPROM
//!
//!     通信接口模块
//!
//!     RS232/RS485串口通信
//!     I2S音频编解码接口
//!     USB转串口芯片CH340
//!
//! ## 关键要点
//!
//!     多协议通信支持：集成Wi-Fi/蓝牙、RS232/RS485、I2C/SPI/I2S等接口，满足物联网设备多样化通信需求。
//!     模块化硬件设计：通过跳线帽实现功能选择，支持摄像头、LCD、无线模块等外设的灵活配置。
//!     低功耗边缘计算：ESP32-S3内置AI加速器可处理图像识别、语音识别等任务，降低云端依赖。
//!     扩展接口丰富：提供GPIO扩展、ADC输入、PWM输出等接口，支持二次开发与功能扩展。
//!
//!
//! # ESP32-S3 正点原子开发板主程序
//!
//! 该程序演示了在正点原子 ESP32-S3 开发板上使用 XL9555 GPIO 扩展芯片控制 LCD 模块的功能。
//!
//! ## 硬件连接说明
//!
//! ### I2C 接口 (用于 XL9555 通信)
//! - SDA: IO41 (GPIO41)
//! - SCL: IO42 (GPIO42)
//!
//! ### SPI 接口 (用于 LCD 通信)
//! - MOSI: IO11 (GPIO11)
//! - SCK:  IO12 (GPIO12)
//! - MISO: IO13 (GPIO13)
//! - CS:   IO21 (GPIO21)
//! - DC:   IO40 (GPIO40)
//!
//! ### XL9555 GPIO 扩展功能
//! - P1.3: LCD 背光控制 (连接到 ATK-MD0240 模块的 PWR 引脚)
//! - P1.2: LCD 复位控制
//! - P1.7-P1.4: 按键输入 (KEY0-KEY3)
//! - P0.3: 蜂鸣器控制输出
//!
//! ### 按键功能
//! - KEY0: 未分配特定功能
//! - KEY1: 切换 LCD 背光状态
//! - KEY2: 未分配特定功能
//! - KEY3: 切换蜂鸣器状态
//!
//! ## 功能说明
//!
//! 1. 初始化 ESP32-S3 系统时钟和外设
//! 2. 初始化 XL9555 GPIO 扩展芯片
//! 3. 初始化 ATK-MD0240 LCD 模块
//! 4. 开启 LCD 背光
//! 5. 启动按键检测任务
//!
//! ## 使用方法
//!
//! 1. 烧录程序到开发板
//! 2. 程序启动后 LCD 背光会自动开启
//! 3. 按下 KEY1 可切换 LCD 背光的开/关状态
//! 4. 按下 KEY3 可切换蜂鸣器的开/关状态

#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

extern crate alloc;

use defmt::{info, warn};
use embassy_executor::Spawner;
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
// 保留以引入panic handler
#[allow(unused)]
use {esp_backtrace, esp_println};

use crate::camera::CameraFrame;
use crate::spi_lcd::ST7789;
use embedded_graphics::prelude::RgbColor;

mod ap3216c;
mod backlight;
mod button;
mod camera;
mod console;
mod dht11;
mod i2c;
mod led;
mod led_flash;
mod ov5640;
mod qma6100p;
mod rgb_lcd;
mod sensor;
mod spi;
mod spi_lcd;
mod wifi;
mod xl9555;

// 创建 esp-idf bootloader 所需的默认应用程序描述符
// 更多信息请参见: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

/// 主函数
///
/// 系统启动入口点，负责初始化所有外设并启动相关任务
#[esp_rtos::main]
async fn main(spawner: Spawner) {
    // generator version: 0.6.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!( size : 64 * 1024 );

    let time_g0_timer = peripherals.TIMG0;
    let time_g0 = TimerGroup::new(time_g0_timer);
    esp_rtos::start(time_g0.timer0);

    info!("Embassy initialized!");

    // 初始化 LED0 (GPIO1)
    // led::led0_init(peripherals.GPIO1).await;

    // 初始化 BOOT 按键 (GPIO0)
    // button::boot_button_init(peripherals.GPIO0).await;

    // 初始化 WiFi
    let result = wifi::init(peripherals.WIFI).await;
    if result.is_err() {
        info!("Failed to initialize WiFi");
    } else {
        let result = spawner.spawn(wifi::wifi_scan());
        if result.is_err() {
            info!("Failed to scan WiFi");
        }
    }

    // dht11::dht11_init(peripherals.GPIO0).await;
    // let result = spawner.spawn(dht11::dht11_task());
    // if result.is_err() {
    //     info!("Failed to spawn dht11 task");
    // }

    // 初始化 RGB LCD
    // 首先配置LCD_CAM外设和DMA通道
    // let lcd_cam = peripherals.LCD_CAM;
    // let dma_channel = peripherals.DMA_CH0;

    // // 配置控制引脚 (根据正点原子开发板实际连接)
    // let de = peripherals.GPIO4;    // DE (Data Enable) - 连接到Camera D0
    // let hsync = peripherals.GPIO5; // HSYNC (Horizontal Sync) - 连接到Camera D1
    // let vsync = peripherals.GPIO47; // VSYNC (Vertical Sync) - 开发板上的VSYNC引脚
    // let pclk = peripherals.GPIO45;  // PCLK (Pixel Clock) - 开发板上的PCLK引脚

    // // 配置数据引脚 (根据正点原子开发板实际连接)
    // // 注意：这些引脚与Camera共享，但在不同时间使用
    // let data_pins: [Option<AnyPin>; 16] = [
    //     Some(peripherals.GPIO4.into()),   // DATA0 - B0 (Camera D0)
    //     Some(peripherals.GPIO5.into()),   // DATA1 - B1 (Camera D1)
    //     Some(peripherals.GPIO6.into()),   // DATA2 - B2 (Camera D2)
    //     Some(peripherals.GPIO7.into()),   // DATA3 - B3 (Camera D3)
    //     Some(peripherals.GPIO15.into()),  // DATA4 - B4 (Camera D4)
    //     Some(peripherals.GPIO16.into()),  // DATA5 - G0 (Camera D5)
    //     Some(peripherals.GPIO17.into()),  // DATA6 - G1 (Camera D6)
    //     Some(peripherals.GPIO18.into()),  // DATA7 - G2 (Camera D7)
    //     None,  // DATA8 - G3 (未连接)
    //     None,  // DATA9 - G4 (未连接)
    //     None,  // DATA10 - G5 (未连接)
    //     None,  // DATA11 - G6 (未连接)
    //     None,  // DATA12 - G7 (未连接)
    //     None,  // DATA13 - R0 (未连接)
    //     None,  // DATA14 - R1 (未连接)
    //     None,  // DATA15 - R2 (未连接)
    // ];

    // // 创建RGB LCD设备实例 (这里使用常见的800x480屏幕ID 0x4384)
    // let rgb_dev = rgb_lcd::RgbLcdDev::new(800, 480, 0x4384);

    // // 初始化RGB LCD
    // let mut rgb_lcd = rgb_lcd::RgbLcd::new(
    //     lcd_cam,
    //     dma_channel,
    //     rgb_dev,
    //     de,
    //     hsync,
    //     vsync,
    //     pclk,
    //     data_pins,
    // );

    // info!(
    //     "RGB LCD initialized with {}x{} resolution",
    //     rgb_lcd.width(),
    //     rgb_lcd.height()
    // );

    // 初始化 XL9555 GPIO 扩展芯片
    // 使用 I2C0 接口，SDA 连接 GPIO41，SCL 连接 GPIO42
    i2c::init(peripherals.I2C0, peripherals.GPIO41, peripherals.GPIO42).await;
    let result = xl9555::init().await;
    if result.is_err() {
        panic!("Failed to initialize XL9555 GPIO expander");
    }
    // 启动按键检测任务
    let result = spawner.spawn(button::read_keys());
    if result.is_err() {
        warn!("Failed to spawn xl9555 task");
    }

    // 初始化 ATK-MD0240 LCD 模块
    let result = xl9555::init_atk_md0240().await;
    if result.is_err() {
        warn!("Failed to initialize ATK-MD0240 LCD module");
    }
    // 开启 LCD 背光
    // 通过 XL9555 的 P1.3 引脚控制 ATK-MD0240 模块的 PWR 引脚
    // 注意：现在背光控制由RgbLcd驱动内部管理，不再需要手动调用
    let result = xl9555::set_lcd_backlight(true).await;
    if result.is_err() {
        warn!("Failed to set LCD backlight");
    }

    // 等待一段时间确保硬件完全准备好
    embassy_time::Timer::after_millis(100).await;

    // // 开启 RGB LCD 背光（使用新的接口）
    // let result = rgb_lcd.backlight_on().await;
    // if result.is_err() {
    //     warn!("Failed to set RGB LCD backlight");
    // }

    // 配置 SPI 接口引脚
    let sck = peripherals.GPIO12; // SPI 时钟线
    let mosi = peripherals.GPIO11; // SPI 主输出从输入线
    let miso = peripherals.GPIO13; // SPI 主输入从输出线
    let cs = peripherals.GPIO21; // SPI 片选线
    let dc = peripherals.GPIO40; // LCD 数据/命令选择线

    let result = spi::init(peripherals.SPI2, sck, mosi, miso, cs).await;
    if result.is_err() {
        panic!("Failed to initialize SPI interface");
    }

    // 初始化并使用ST7789显示屏
    let mut guard = spi::SPI.lock().await;
    let spi_ref = guard.take().unwrap();

    // 创建ST7789驱动实例 (根据实际情况调整分辨率)
    // 注意: 根据用户确认，ST7789显示屏实际使用的分辨率为240x320
    let mut display = spi_lcd::ST7789::new(
        spi_ref,
        dc,
        Some(peripherals.GPIO14), // 使用硬件复位
        240,                      // 宽度
        320,                      // 高度 (用户确认的正确分辨率)
    );

    // 初始化显示屏
    info!("Initializing ST7789 display...");
    let init_result = display.init();
    if init_result.is_err() {
        warn!(
            "Failed to initialize ST7789 display: {:?}",
            init_result.err()
        );
    }

    // 等待显示初始化完成
    embassy_time::Timer::after_millis(100).await;

    // 使用embedded-graphics绘制图形来测试显示是否正常工作
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::prelude::*;
    use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};

    // 清屏为蓝色背景来确认显示是否工作
    let _ = display.clear(Rgb565::BLUE);

    // 绘制几个不同颜色的矩形来测试颜色显示
    let red_square = Rectangle::new(Point::new(10, 10), Size::new(50, 50))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::RED));
    let _ = red_square.draw(&mut display);

    let green_square = Rectangle::new(Point::new(70, 10), Size::new(50, 50))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN));
    let _ = green_square.draw(&mut display);

    let blue_square = Rectangle::new(Point::new(130, 10), Size::new(50, 50))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLUE));
    let _ = blue_square.draw(&mut display);

    // 绘制黄色和洋红色方块来进一步测试颜色混合
    let yellow_square = Rectangle::new(Point::new(10, 70), Size::new(50, 50))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW));
    let _ = yellow_square.draw(&mut display);

    let magenta_square = Rectangle::new(Point::new(70, 70), Size::new(50, 50))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::MAGENTA));
    let _ = magenta_square.draw(&mut display);

    let cyan_square = Rectangle::new(Point::new(130, 70), Size::new(50, 50))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::CYAN));
    let _ = cyan_square.draw(&mut display);

    // 等待一段时间观察屏幕显示
    embassy_time::Timer::after_millis(2000).await;

    // 初始化摄像头
    info!("Initializing OV5640 camera...");

    // 创建OV5640配置 - 使用PSRAM优化配置和QVGA分辨率
    let mut ov5640_config = ov5640::OV5640Config::with_power_scheme_a();

    // 配置摄像头引脚（根据正点原子开发板实际连接设置）
    ov5640_config.base_config.pins = camera::CameraPins {
        pin_pwdn: Some(peripherals.GPIO0.into()), // PWDN引脚，通过XL9555的P0.4控制
        pin_reset: Some(peripherals.GPIO1.into()), // RESET引脚，通过XL9555的P0.5控制
        pin_xclk: peripherals.GPIO3.into(),       // XCLK引脚，连接模块自带的24MHz晶振
        pin_sccb_sda: peripherals.GPIO39.into(),  // SCCB SDA引脚 (OV_SDA)
        pin_sccb_scl: peripherals.GPIO38.into(),  // SCCB SCL引脚 (OV_SCL)
        pin_d7: peripherals.GPIO18.into(),        // 数据位7
        pin_d6: peripherals.GPIO17.into(),        // 数据位6
        pin_d5: peripherals.GPIO16.into(),        // 数据位5
        pin_d4: peripherals.GPIO15.into(),        // 数据位4
        pin_d3: peripherals.GPIO7.into(),         // 数据位3
        pin_d2: peripherals.GPIO6.into(),         // 数据位2
        pin_d1: peripherals.GPIO5.into(),         // 数据位1
        pin_d0: peripherals.GPIO4.into(),         // 数据位0
        pin_vsync: peripherals.GPIO47.into(),     // 垂直同步
        pin_href: peripherals.GPIO48.into(),      // 行参考 (HREF)
        pin_pclk: peripherals.GPIO45.into(),      // 像素时钟
    };
    ov5640_config.base_config.xclk_freq_hz = 24_000_000; // 设置XCLK频率为24MHz，匹配模块晶振
    // 使用更小的分辨率以减少内存使用
    ov5640_config.base_config.frame_size = camera::FrameSize::Qvga; // 320x240
    ov5640_config.base_config.pixel_format = camera::PixelFormat::Rgb565;
    ov5640_config.base_config.fb_location = camera::FrameBufferLocation::InPsram;
    ov5640_config.base_config.fb_count = 1; // 使用单缓冲以减少内存使用

    // 创建OV5640摄像头实例
    let mut ov5640_camera = ov5640::OV5640Camera::new(ov5640_config);

    // 初始化摄像头
    match ov5640_camera.init().await {
        Ok(()) => {
            info!("OV5640 camera initialized successfully");

            // 测试摄像头性能（使用更少的帧数以减少内存压力）
            match ov5640_camera.test_performance(2).await {
                Ok(performance) => {
                    info!(
                        "Camera performance test: {} FPS, avg frame size: {} bytes",
                        performance.fps, performance.avg_size
                    );
                }
                Err(e) => {
                    warn!("Camera performance test failed: {}", e);
                }
            }

            // 创建一次性的LED闪光灯控制器用于首次拍照
            let mut led_flash = led_flash::LEDFlashController::new();
            
            // 初始化LED闪光灯（只需要初始化一次）
            info!("Initializing LED flash...");
            if let Err(e) = led_flash.init().await {
                warn!("Failed to initialize LED flash: {}", e);
            } else {
                info!("LED flash initialized successfully");
                
                // 开机自动拍照并使用闪光灯
                info!("Taking auto photo with flash...");
                
                // 触发闪光灯
                info!("Triggering flash...");
                if let Err(e) = led_flash.trigger().await {
                    warn!("Failed to trigger flash: {}", e);
                } else {
                    info!("Flash triggered successfully");
                }

                // 等待一段时间确保闪光灯触发
                embassy_time::Timer::after_millis(200).await;
            }

            // 捕获一帧图像
            info!("Capturing frame...");
            match ov5640_camera.capture_frame().await {
                Ok(frame) => {
                    info!("Frame captured successfully, size: {}x{}", frame.width, frame.height);
                    
                    // 在SPI LCD上显示图像
                    if let Err(e) = display_image(&mut display, &frame).await {
                        warn!("Failed to display image: {:?}", e);
                    } else {
                        info!("Image displayed successfully");
                    }
                    
                    // 释放帧缓冲区内存
                    ov5640_camera.release_frame(frame);
                    
                    info!("Auto photo taken and displayed successfully");
                }
                Err(e) => {
                    warn!("Failed to capture auto photo: {}", e);
                }
            }

            // 启动摄像头显示任务
            // 注意：SPI LCD的显示任务与RGB LCD不同，需要专门适配
            let result = spawner.spawn(camera_display_task(ov5640_camera, display));
            if result.is_err() {
                warn!("Failed to spawn camera display task");
            }
        }
        Err(e) => {
            warn!("Failed to initialize OV5640 camera: {}", e);
        }
    }
}

/// 摄像头显示任务
///
/// 这个任务持续从摄像头捕获图像并在SPI LCD上显示
#[embassy_executor::task]
async fn camera_display_task(
    mut camera: ov5640::OV5640Camera<'static>,
    mut display: ST7789<'static>,
) {
    // 创建LED闪光灯控制器（只需要创建一次）
    let mut led_flash = led_flash::LEDFlashController::new();
    
    // 初始化LED闪光灯（只需要初始化一次）
    if let Err(e) = led_flash.init().await {
        defmt::warn!("Failed to initialize LED flash: {}", e);
    }

    loop {
        // 捕获一帧图像
        match camera.capture_frame().await {
            Ok(frame) => {
                // 触发闪光灯
                if let Err(e) = led_flash.trigger().await {
                    defmt::warn!("Failed to trigger flash: {}", e);
                }
                
                // 在SPI LCD上显示图像
                let _ = display_image(&mut display, &frame).await;
                // 释放帧缓冲区内存
                camera.release_frame(frame);
            }
            Err(e) => {
                defmt::warn!("Failed to capture frame: {}", e);
            }
        }

        // 等待一小段时间再捕获下一帧
        embassy_time::Timer::after_secs(10).await;
    }
}

/// 在SPI LCD上显示图像
async fn display_image(
    display: &mut ST7789<'_>,
    frame: &CameraFrame,
) -> Result<(), esp_hal::spi::Error> {
    // 清屏为黑色
    use embedded_graphics::pixelcolor::Rgb565;

    // 先清屏确保没有残留的白色区域
    display.fill_screen(Rgb565::BLACK)?;

    // 简单的图像显示实现
    // 注意：这是一个简化的实现，实际应用中可能需要更好的缩放算法
    if frame.width == 240 && frame.height == 320 {
        // 图像尺寸正好适合屏幕旋转后的尺寸
        // 我们需要将RGB565数据写入LCD
        write_frame(display, frame)?;
    } else {
        // 其他尺寸，简单填充屏幕中心区域
        display.fill_rectangle(0, 0, 240, 320, Rgb565::BLUE)?;
    }

    Ok(())
}

/// 将帧数据写入LCD显示
fn write_frame(
    display: &mut spi_lcd::ST7789<'_>,
    frame: &camera::CameraFrame,
) -> Result<(), esp_hal::spi::Error> {
    // 设置显示窗口为整个屏幕区域
    // 注意：对于240x320的屏幕，坐标范围是0-239和0-319
    display.set_address_window(0, 0, 239, 319)?;

    // 发送RAM写入命令
    display.write_command(0x2C, &[])?;

    // 设置为数据模式
    display.dc.set_high();

    // 发送图像数据
    // 注意：这里简化处理，实际应该进行适当的格式转换
    if !frame.data.is_empty() {
        // 对于RGB565数据，我们需要确保字节顺序正确
        display.spi_mut().write(&frame.data)?;
    }

    Ok(())
}
