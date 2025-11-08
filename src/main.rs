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
//!
//! ### 按键功能
//! - KEY0: 未分配特定功能
//! - KEY1: 切换 LCD 背光状态
//! - KEY2: 未分配特定功能
//! - KEY3: 未分配特定功能
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

mod button;
mod i2c;
mod lcd;
mod led;
mod spi;
mod st7789;
mod wifi;
mod xl9555;

// 创建 esp-idf bootloader 所需的默认应用程序描述符
// 更多信息请参见: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::RgbColor;

#[esp_rtos::main]
/// 主函数
///
/// 系统启动入口点，负责初始化所有外设并启动相关任务
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
    led::led0_init(peripherals.GPIO1).await;

    // 初始化 BOOT 按键 (GPIO0)
    button::boot_button_init(peripherals.GPIO0).await;

    // 初始化 WiFi
    let result = wifi::init(peripherals.WIFI).await;
    if result.is_err() {
        info!("Failed to initialize WiFi");
    } else {
        let result = spawner.spawn(wifi::wifi_scan());
        if result.is_err() {
            info!("Failed to initialize WiFi");
        }
    }

    // 初始化 XL9555 GPIO 扩展芯片
    // 使用 I2C0 接口，SDA 连接 GPIO41，SCL 连接 GPIO42
    i2c::init(peripherals.I2C0, peripherals.GPIO41, peripherals.GPIO42).await;
    let result = xl9555::init().await;
    if result.is_err() {
        info!("Failed to initialize XL9555 GPIO expander");
    } else {
        // 启动按键检测任务
        let result = spawner.spawn(xl9555::read_keys());
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
        let result = xl9555::set_lcd_backlight(true).await;
        if result.is_err() {
            warn!("Failed to set LCD backlight");
        }
    }

    // 配置 SPI 接口引脚
    let sck = peripherals.GPIO12; // SPI 时钟线
    let mosi = peripherals.GPIO11; // SPI 主输出从输入线
    let miso = peripherals.GPIO13; // SPI 主输入从输出线
    let cs = peripherals.GPIO21; // SPI 片选线
    let dc = peripherals.GPIO40; // LCD 数据/命令选择线

    let result = spi::init(peripherals.SPI2, sck, mosi, miso).await;

    if result.is_err() {
        warn!("Failed to initialize SPI interface");
    } else {
        // 初始化并使用ST7789显示屏
        let mut guard = spi::SPI.lock().await;
        let spi_ref = guard.take().unwrap();

        // 创建ST7789驱动实例 (240x135 是ST7789常见的分辨率)
        let mut display = st7789::ST7789::new(
            spi_ref,
            dc,
            Option::<esp_hal::gpio::AnyPin>::None, // 使用软件复位
            320,                                   // 宽度
            240,                                   // 高度
        );

        // 初始化显示屏
        let init_result = display.init();
        if init_result.is_err() {
            warn!(
                "Failed to initialize ST7789 display: {:?}",
                init_result.err()
            );
        } else {
            info!("ST7789 display initialized successfully");

            // 填充屏幕为红色
            let fill_result = display.fill_screen(Rgb565::RED);
            if fill_result.is_err() {
                warn!("Failed to fill screen with red color");
            } else {
                info!("Screen filled with red color");
            }

            // 等待一段时间
            embassy_time::Timer::after_millis(1000).await;

            // 填充屏幕为绿色
            let fill_result = display.fill_screen(Rgb565::GREEN);
            if fill_result.is_err() {
                warn!("Failed to fill screen with green color");
            } else {
                info!("Screen filled with green color");
            }

            // 等待一段时间
            embassy_time::Timer::after_millis(1000).await;

            // 填充屏幕为蓝色
            let fill_result = display.fill_screen(Rgb565::BLUE);
            if fill_result.is_err() {
                warn!("Failed to fill screen with blue color");
            } else {
                info!("Screen filled with blue color");
            }
        }
    }
}
