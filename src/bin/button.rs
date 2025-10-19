use defmt::info;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as EmbassyMutex;
use esp_hal::gpio::{Event, Input, InputConfig, InputPin};

pub static BOOT_BUTTON_ASYNC: EmbassyMutex<CriticalSectionRawMutex, Option<Input<'static>>> =
    EmbassyMutex::new(None);
pub async fn boot_button_init(button: impl InputPin + 'static) {
    let mut boot_button = Input::new(button, InputConfig::default());
    boot_button.listen(Event::FallingEdge);
    BOOT_BUTTON_ASYNC.lock().await.replace(boot_button);
    info!("Boot button initialized")
}
