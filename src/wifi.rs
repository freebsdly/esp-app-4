use defmt::{info, warn};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as EmbassyMutex;
use esp_hal::peripherals::WIFI;
use esp_radio::wifi::ModeConfig::Client;
use esp_radio::wifi::{ClientConfig, Config as WifiConfig, ScanConfig, WifiController};
use esp_radio::Controller;
use static_cell::StaticCell;

static RADIO_INIT: StaticCell<Controller> = StaticCell::new();
static WIFI_CONTROLLER: EmbassyMutex<CriticalSectionRawMutex, Option<WifiController<'static>>> =
    EmbassyMutex::new(None);

pub async fn init(peripherals_wifi: WIFI<'static>) -> Result<(), esp_radio::InitializationError> {
    let radio_init = esp_radio::init()?;
    let radio_init_ref = RADIO_INIT.init(radio_init);

    let (mut wifi_controller, _interfaces) =
        esp_radio::wifi::new(radio_init_ref, peripherals_wifi, WifiConfig::default())?;

    wifi_controller.set_config(&Client(ClientConfig::default()))?;
    wifi_controller.start_async().await?;
    let result = wifi_controller.is_started();
    if result.is_err() {
        warn!("Wi-Fi is not started: {}", result.err().unwrap())
    }
    WIFI_CONTROLLER.lock().await.replace(wifi_controller);
    Ok(())
}

#[embassy_executor::task]
pub async fn wifi_scan() {
    info!("Wifi Scanning...");

    let mut guard = WIFI_CONTROLLER.lock().await;
    if let Some(controller) = guard.as_mut() {
        let scan_config = ScanConfig::default().with_max(10);
        let result = controller.scan_with_config_async(scan_config).await;

        match result {
            Ok(networks) => {
                info!("Scan done, found {} networks", networks.len());
                for network in networks {
                    info!(
                        "SSID: {}, Channel: {}, RSSI: {}",
                        core::str::from_utf8((&network.ssid).as_ref()).unwrap_or("<invalid utf-8>"),
                        network.channel,
                        network.auth_method
                    );
                }
            }
            Err(err) => {
                warn!("Wi-Fi scan failed: {}", err);
            }
        }
    }
}
