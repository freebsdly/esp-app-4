use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as EmbassyMutex;
use esp_hal::dma::{DmaChannelFor, DmaRxBuf, DmaTxBuf};
use esp_hal::gpio::interconnect::{PeripheralInput, PeripheralOutput};
use esp_hal::spi::master::{AnySpi, ConfigError, Instance};
use esp_hal::spi::master::{Config, Spi, SpiDmaBus};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use esp_hal::Blocking;

pub static SPI_WITH_DMA: EmbassyMutex<CriticalSectionRawMutex, Option<SpiDmaBus<Blocking>>> =
    EmbassyMutex::new(None);

pub static SPI: EmbassyMutex<CriticalSectionRawMutex, Option<Spi<Blocking>>> =
    EmbassyMutex::new(None);

#[allow(unused)]
pub async fn init_with_dma(
    spi2: impl Instance + 'static,
    sck: impl PeripheralOutput<'static>,
    mos: impl PeripheralOutput<'static>,
    mis: impl PeripheralInput<'static>,
    dma_channel: impl DmaChannelFor<AnySpi<'static>>,
    dma_rx_buf: DmaRxBuf,
    dma_tx_buf: DmaTxBuf,
) -> Result<(), ConfigError> {
    // 初始化 SPI 接口
    let spi = Spi::new(
        spi2,
        Config::default()
            .with_frequency(Rate::from_mhz(10))
            .with_mode(Mode::_0),
    )?
    .with_sck(sck)
    .with_mosi(mos)
    .with_miso(mis)
    .with_dma(dma_channel)
    .with_buffers(dma_rx_buf, dma_tx_buf);

    SPI_WITH_DMA.lock().await.replace(spi);
    Ok(())
}

#[allow(unused)]
pub async fn init(
    spi2: impl Instance + 'static,
    sck: impl PeripheralOutput<'static>,
    mos: impl PeripheralOutput<'static>,
    mis: impl PeripheralInput<'static>,
) -> Result<(), ConfigError> {
    // 初始化 SPI 接口
    let spi = Spi::new(
        spi2,
        Config::default()
            .with_frequency(Rate::from_mhz(10))
            .with_mode(Mode::_0),
    )?
    .with_sck(sck)
    .with_mosi(mos)
    .with_miso(mis);

    SPI.lock().await.replace(spi);
    Ok(())
}
