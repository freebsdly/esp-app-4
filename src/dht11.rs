//! DHT11 数字温湿度传感器驱动
//!
//! DHT11是一款数字温湿度传感器，采用单总线协议进行数据传输。
//! 数据格式为40位数据：湿度整数+湿度小数+温度整数+温度小数+校验和。
//! 本模块提供了读取DHT11传感器数据的功能。

use defmt::{info, warn};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as EmbassyMutex;
use embassy_time::Timer;
use esp_hal::gpio::Level;
use esp_hal::gpio::{Flex, InputConfig, InputPin, OutputConfig, OutputPin, Pull};
use esp_hal::delay::Delay;

/// DHT11传感器读取错误类型
#[derive(Debug)]
pub enum Dht11Error {
    /// 校验和错误
    ChecksumMismatch,
    /// 传感器无响应
    NoResponse,
    /// 信号超时
    Timeout,
}

// 为Dht11Error实现defmt::Format trait，以便可以使用defmt打印错误
impl defmt::Format for Dht11Error {
    fn format(&self, f: defmt::Formatter) {
        match self {
            Dht11Error::ChecksumMismatch => {
                defmt::write!(f, "Checksum mismatch");
            }
            Dht11Error::NoResponse => {
                defmt::write!(f, "No response from sensor");
            }
            Dht11Error::Timeout => {
                defmt::write!(f, "Timeout waiting for signal");
            }
        }
    }
}

/// 温湿度数据
#[derive(Debug, Clone, Copy)]
pub struct Dht11Data {
    /// 湿度整数部分 (%RH)
    pub humidity_integral: u8,
    /// 湿度小数部分 (%RH)
    pub humidity_decimal: u8,
    /// 温度整数部分 (°C)
    pub temperature_integral: u8,
    /// 温度小数部分 (°C)
    pub temperature_decimal: u8,
}

impl Dht11Data {
    /// 获取湿度值 (%RH)
    pub fn humidity(&self) -> f32 {
        self.humidity_integral as f32 + (self.humidity_decimal as f32 / 10.0)
    }

    /// 获取温度值 (°C)
    pub fn temperature(&self) -> f32 {
        self.temperature_integral as f32 + (self.temperature_decimal as f32 / 10.0)
    }
}

impl defmt::Format for Dht11Data {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(
            f,
            "Dht11Data {{ humidity: {}.{}, temperature: {}.{} }}",
            self.humidity_integral,
            self.humidity_decimal,
            self.temperature_integral,
            self.temperature_decimal
        );
    }
}

/// DHT11引脚静态变量
static DHT11_PIN: EmbassyMutex<CriticalSectionRawMutex, Option<Flex<'static>>> =
    EmbassyMutex::new(None);

/// 初始化DHT11传感器引脚
pub async fn dht11_init(pin: impl OutputPin + InputPin + 'static) {
    let flex_pin = Flex::new(pin);
    DHT11_PIN.lock().await.replace(flex_pin);
    info!("DHT11 init done");
}

/// 读取DHT11传感器数据
///
/// # 返回值
/// * `Ok(Dht11Data)` - 成功读取的温湿度数据
/// * `Err(Dht11Error)` - 读取过程中发生的错误
pub async fn read_dht11() -> Result<Dht11Data, Dht11Error> {
    // 获取DHT11引脚
    let mut guard = DHT11_PIN.lock().await;
    let flex_pin = guard.as_mut().unwrap();

    // 创建硬件级延时实例
    let mut delay = Delay::new();

    // 初始化总线（高电平稳定）
    flex_pin.set_high();
    flex_pin.apply_output_config(
        &OutputConfig::default()
            .with_pull(Pull::Up)
            .with_drive_strength(esp_hal::gpio::DriveStrength::_20mA),
    );
    flex_pin.set_output_enable(true);
    delay.delay_micros(10000); // 10ms 高电平，稳定总线

    // 步骤 1：主机发起请求
    // 主机将数据线拉低 20ms
    flex_pin.set_low();
    delay.delay_micros(20000); // 20ms

    // 然后释放（上拉）总线并切换为输入模式
    flex_pin.set_high();
    flex_pin.set_input_enable(true);
    flex_pin.apply_input_config(&InputConfig::default().with_pull(Pull::None)); // 高阻输入
    delay.delay_micros(40); // 等待传感器响应

    // 步骤 2：DHT11 响应
    // 等待传感器响应（80us 低电平，80us 高电平）
    wait_for_level(&mut *flex_pin, Level::Low, 200, &mut delay)?;
    wait_for_level(&mut *flex_pin, Level::High, 200, &mut delay)?;

    // 步骤 3：数据传输（共 40 位）
    // DHT11 发送 40 位数据，格式如下：
    // [湿度高位] [湿度低位] [温度高位] [温度低位] [校验和]
    // 每个字节 8 位，共 5 字节
    let mut data = [0u8; 5];
    for byte in &mut data {
        *byte = read_byte(flex_pin, &mut delay)?;
    }

    // 等待结束
    wait_for_level(flex_pin, Level::Low, 100, &mut delay)?;

    // 校验数据
    // 校验和 = 湿度高位 + 湿度低位 + 温度高位 + 温度低位
    let checksum = data[0]
        .wrapping_add(data[1])
        .wrapping_add(data[2])
        .wrapping_add(data[3]);
    if checksum != data[4] {
        return Err(Dht11Error::ChecksumMismatch);
    }

    // 恢复总线为高电平
    flex_pin.set_output_enable(true);
    flex_pin.set_high();

    // 返回解析后的数据
    Ok(Dht11Data {
        humidity_integral: data[0],
        humidity_decimal: data[1],
        temperature_integral: data[2],
        temperature_decimal: data[3],
    })
}

/// DHT11传感器驱动
pub struct DHT11;

impl DHT11 {
    /// 创建一个新的DHT11传感器实例
    pub fn new() -> Self {
        Self
    }

    /// 读取DHT11传感器数据
    ///
    /// # 返回值
    /// * `Ok(Dht11Data)` - 成功读取的温湿度数据
    /// * `Err(Dht11Error)` - 读取过程中发生的错误
    pub async fn read(&self) -> Result<Dht11Data, Dht11Error> {
        read_dht11().await
    }
}

/// 读取一个字节的数据
///
/// # 返回值
/// * `Ok(u8)` - 成功读取的字节
/// * `Err(Dht11Error)` - 读取过程中发生的错误
fn read_byte(pin: &mut Flex<'_>, delay: &mut Delay) -> Result<u8, Dht11Error> {
    let mut data = 0u8;

    for i in 0..8 {
        // 1. 等待数据位的低电平（固定50us）
        wait_for_level(pin, Level::Low, 100, delay)?;
        
        // 2. 等待高电平开始（传感器拉低结束）
        wait_for_level(pin, Level::High, 100, delay)?;
        
        // 3. 延时45us（临界点：28us < 45us < 70us）
        delay.delay_micros(45);
        
        // 4. 读取当前电平：高1，低0
        if pin.level() == Level::High {
            data |= 1 << (7 - i); // 高位在前
        }
        
        // 5. 等待当前数据位的高电平结束（避免影响下一位）
        wait_for_level(pin, Level::Low, 100, delay)?;
    }

    Ok(data)
}

/// 等待引脚达到目标电平，超时返回错误（单位：微秒）
fn wait_for_level(
    pin: &mut Flex<'_>,
    target_level: Level,
    timeout_us: u32,
    delay: &mut Delay,
) -> Result<(), Dht11Error> {
    for _ in 0..timeout_us {
        if pin.level() == target_level {
            return Ok(());
        }
        delay.delay_micros(1);
    }
    Err(Dht11Error::Timeout)
}

/// DHT11传感器任务，定期读取并打印温湿度数据
#[embassy_executor::task]
pub async fn dht11_task() {
    let dht11 = DHT11::new();
    loop {
        match dht11.read().await {
            Ok(data) => {
                info!(
                    "Temperature: {}°C, Humidity: {}%RH",
                    data.temperature(),
                    data.humidity()
                );
            }
            Err(e) => {
                warn!("Failed to read DHT11 sensor: {:?}", e);
            }
        }

        // DHT11传感器每次读取后必须等待至少2秒才能进行下一次读取
        Timer::after_secs(2).await;
    }
}