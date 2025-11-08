use defmt::info;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as EmbassyMutex;
use esp_hal::gpio::{Level, Output, OutputConfig, OutputPin};

pub static LED0: EmbassyMutex<CriticalSectionRawMutex, Option<Output<'static>>> =
    EmbassyMutex::new(None);
pub async fn led0_init(led: impl OutputPin + 'static) {
    // 分配 GPIO 引脚
    let mut led0 = Output::new(led, Level::Low, OutputConfig::default());
    led0.set_low();
    LED0.lock().await.replace(led0);
    info!("LED0 init done");
}

#[allow(unused)]
pub async fn led0_toggle() {
    if let Some(led0) = LED0.lock().await.as_mut() {
        led0.toggle();
    }
}
