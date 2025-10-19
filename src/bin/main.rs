#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use {esp_backtrace as _, esp_println as _};

mod button;
mod led;
mod wifi;

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 0.6.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[unsafe(link_section = ".dram2_uninit")] size: 73744);

    let time_g0_timer = peripherals.TIMG0;
    let time_g0 = TimerGroup::new(time_g0_timer);
    esp_rtos::start(time_g0.timer0);

    info!("Embassy initialized!");

    led::led0_init(peripherals.GPIO1).await;

    button::boot_button_init(peripherals.GPIO0).await;

    // Initialize WiFi
    wifi::init(peripherals.WIFI).await;
    spawner.spawn(wifi::wifi_scan()).ok();

    loop {
        led::led0_toggle().await;
        Timer::after(Duration::from_secs(1)).await;
    }
}
