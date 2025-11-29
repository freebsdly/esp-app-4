//! DHT11 数字温湿度传感器驱动
//!
//! DHT11是一款数字温湿度传感器，采用单总线协议进行数据传输。
//! 数据格式为40位数据：湿度整数+湿度小数+温度整数+温度小数+校验和。
//! 本模块提供了读取DHT11传感器数据的功能。

use defmt::{info, warn};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as EmbassyMutex;
use embassy_time::Timer;
use esp_hal::delay::Delay;
use esp_hal::gpio::Level;
use esp_hal::gpio::{Flex, InputConfig, InputPin, OutputConfig, OutputPin, Pull};

/// DHT11传感器读取错误类型
#[derive(Debug)]
pub enum Dht11Error {
    // 校验和错误
    ChecksumMismatch,
    // 传感器无响应
    NoResponse,
    // 信号超时
    Timeout,
    // 未期待的电平
    UnExpectedPinLevel,
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
            Dht11Error::UnExpectedPinLevel => {
                defmt::write!(f, "unexpected pin level error");
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

    // === 关键修复1：使用开漏输出模式 ===
    // 配置为开漏输出，配合外部上拉电阻（参考C版本GPIO_MODE_INPUT_OUTPUT_OD）
    flex_pin.apply_output_config(
        &OutputConfig::default()
            .with_pull(Pull::None)
            .with_drive_mode(esp_hal::gpio::DriveMode::OpenDrain),
    );
    flex_pin.set_output_enable(true);
    flex_pin.set_high();

    // 步骤2：主机发起请求（拉低20ms）
    flex_pin.set_low();
    delay.delay_millis(19);

    // 2. 主机释放总线（输出高电平），让上拉电阻工作
    flex_pin.set_high();
    delay.delay_micros(32); // 主机拉高10-35us（参考C版本）

    // 3. 切换到输入模式等待传感器响应
    flex_pin.set_output_enable(false);
    flex_pin.apply_input_config(&InputConfig::default().with_pull(Pull::None));
    flex_pin.set_input_enable(true);

    // 等待传感器响应（80us低电平 + 80us高电平）
    wait_for_with_level(flex_pin, Level::Low, 80, &mut delay)?; // 等待80us低电平
    wait_for_with_level(flex_pin, Level::High, 80, &mut delay)?; // 等待80us高电平
    // 步骤 3：数据传输（共 40 位）
    // DHT11 发送 40 位数据，格式如下：
    // [湿度整数] [湿度小数] [温度整数] [温度小数] [校验和]
    // 校验和数据等于“8bit湿度整数数据+8bit湿度小数数据 +8bi温度整数数据+8bit温度小数数据”所得结果的末8位
    // 每个字节 8 位，共 5 字节

    let mut data = [0u8; 5];
    for byte in &mut data {
        *byte = read_byte(flex_pin, &mut delay)?;
    }

    // 等待结束
    wait_for_level(flex_pin, Level::Low, 100, &mut delay)?;

    // 在读取完成后打印原始数据用于调试
    info!(
        "Raw data: [{}, {}, {}, {}, {}]",
        data[0], data[1], data[2], data[3], data[4]
    );

    // 校验数据
    // 校验和 = 湿度高位 + 湿度低位 + 温度高位 + 温度低位
    // 增强校验和验证
    let calculated_checksum = data
        .iter()
        .take(4)
        .fold(0u8, |sum, &val| sum.wrapping_add(val));

    if calculated_checksum != data[4] {
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
    let mut byte = 0u8;

    for i in 0..8 {
        // 等待高电平开始（每个bit以50us低电平开始，上升沿算10us)
        wait_for_level(pin, Level::High, 55, delay)?;
        // 上升沿， 等待电平稳定
        delay.delay_micros(5);
        if pin.is_high() {
            // bit=0 时高电平持续26-28us
            // bit=1 时高电平持续70us
            // 加上下降沿 10us
            delay.delay_micros(40);
            if pin.is_high() {
                // 说明仍处于高电平，则bit=1
                byte |= 1 << (7 - i);
                delay.delay_micros(30);
            } else {
                // 已经是低电平
                // 说明bit=0
            }
        } else {
            // TODO: 50us低电平过后应该为高电平，否则数据有误
            return Err(Dht11Error::UnExpectedPinLevel);
        }
    }
    Ok(byte)
}

/// 等待引脚达到目标电平，超时返回错误（单位：微秒）
fn wait_for_level(
    pin: &mut Flex<'_>,
    target_level: Level,
    timeout_us: u32,
    delay: &mut Delay,
) -> Result<(), Dht11Error> {
    let step_us = 1u32; // 更精细的1us间隔检测
    let mut waited = 0u32;

    while waited < timeout_us {
        if pin.level() == target_level {
            return Ok(());
        }
        delay.delay_micros(step_us);
        waited += step_us;
    }

    Err(Dht11Error::Timeout)
}

/// 等待引脚处于目标电平指定时间
fn wait_for_with_level(
    pin: &mut Flex<'_>,
    target_level: Level,
    us: u32,
    delay: &mut Delay,
) -> Result<(), Dht11Error> {
    let step_us = 1u32; // 更精细的1us间隔检测
    let mut waited = 0u32;

    while waited < us {
        if pin.level() == target_level {
            delay.delay_micros(step_us);
            waited += step_us;
        } else {
            return Err(Dht11Error::UnExpectedPinLevel);
        }
    }

    Ok(())
}

/// 硬件检查函数（验证外部上拉电阻）
pub async fn check_hardware_config() -> bool {
    let mut guard = DHT11_PIN.lock().await;
    let flex_pin = guard.as_mut().unwrap();
    let delay = Delay::new();

    info!("=== DHT11 Hardware Configuration Check ===");

    // 测试外部上拉电阻
    flex_pin.apply_input_config(&InputConfig::default().with_pull(Pull::None));
    flex_pin.set_input_enable(true);
    delay.delay_millis(10);

    let external_pull_level = flex_pin.level();
    info!(
        "Bus level with external pull-up only: {:?}",
        external_pull_level
    );

    // 关键检查：必须有外部上拉电阻将总线拉高
    if external_pull_level != Level::High {
        warn!("CRITICAL: External 5kΩ pull-up resistor is missing or not working!");
        warn!("Please add a 4.7kΩ-5kΩ resistor between DATA pin and VCC");
        return false;
    }

    info!("✓ External pull-up resistor is working correctly");
    info!("=== Hardware Check Complete ===");
    true
}

/// DHT11传感器任务，定期读取并打印温湿度数据
#[embassy_executor::task]
pub async fn dht11_task() {
    // 先检查硬件配置
    if !check_hardware_config().await {
        warn!("DHT11 hardware configuration issue, sensor may not work!");
    }

    let dht11 = DHT11::new();
    loop {
        match dht11.read().await {
            Ok(data) => {
                info!(
                    "Temperature: {:01}°C, Humidity: {:01}%",
                    data.temperature(),
                    data.humidity()
                );
            }
            Err(e) => {
                warn!("DHT11 read error: {:?}", e);
                // 增加错误后的延时
                Timer::after_secs(3).await;
                continue;
            }
        }

        Timer::after_secs(2).await; // DHT11要求至少2秒间隔
    }
}
