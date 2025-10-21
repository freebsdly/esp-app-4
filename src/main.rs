#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use core::cell::RefCell;
use defmt::info;
use display_interface_spi::SPIInterface;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDevice;
use embassy_executor::Spawner;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use embedded_hal::spi::MODE_0;
use esp_hal::clock::CpuClock;
use esp_hal::dma::{DmaRxBuf, DmaTxBuf};
use esp_hal::gpio::Io;
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config, Spi, SpiDma};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{dma_buffers, peripherals};
use mipidsi::Builder;
use mipidsi::models::ST7789;
use {esp_backtrace as _, esp_println as _};

mod button;
mod lcd;
mod led;
mod wifi;
mod xl9555;

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    // generator version: 0.6.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 64 * 1024);

    let time_g0_timer = peripherals.TIMG0;
    let time_g0 = TimerGroup::new(time_g0_timer);
    esp_rtos::start(time_g0.timer0);

    info!("Embassy initialized!");

    led::led0_init(peripherals.GPIO1).await;

    button::boot_button_init(peripherals.GPIO0).await;

    // Initialize WiFi
    wifi::init(peripherals.WIFI).await;
    spawner
        .spawn(wifi::wifi_scan())
        .expect("failed to spawn wifi task");

    xl9555::init(peripherals.I2C0, peripherals.GPIO41, peripherals.GPIO42).await;
    spawner
        .spawn(xl9555::read_keys())
        .expect("failed to spawn xl9555 task");

    // // DMA 缓冲区（发送为主，接收小）
    // let dma_channel = peripherals.DMA_CH0;
    // let (rx_buffer, rx_descriptors, tx_buffer, tx_descriptors) = dma_buffers!(32000);
    //
    // let dma_rx_buf = DmaRxBuf::new(rx_descriptors, rx_buffer).unwrap();
    //
    // let dma_tx_buf = DmaTxBuf::new(tx_descriptors, tx_buffer).unwrap();
    //
    // let mut spi = Spi::new(
    //     peripherals.SPI2,
    //     Config::default()
    //         .with_frequency(Rate::from_khz(100))
    //         .with_mode(Mode::_0),
    // )
    // .unwrap()
    // .with_dma(dma_channel)
    // .with_buffers(dma_rx_buf, dma_tx_buf);

    let sck = peripherals.GPIO12;
    let mos = peripherals.GPIO11;
    let mis = peripherals.GPIO13;
    let cs = peripherals.GPIO21;
    let dc = peripherals.GPIO40;

    let mut spi = Spi::new(
        peripherals.SPI2,
        Config::default()
            .with_frequency(Rate::from_mhz(10))
            .with_mode(Mode::_0),
    )
    .expect("failed to initialize SPI")
    .with_sck(sck)
    .with_mosi(mos)
    .with_miso(mis)
    .with_cs(cs);

    // LCD 复位序列 - 通过 XL9555 控制
    xl9555::spi_lcd_reset(false);  // 复位引脚拉低
    Timer::after(Duration::from_millis(10)).await;
    xl9555::spi_lcd_reset(true);   // 复位引脚拉高
    Timer::after(Duration::from_millis(10)).await;
}
