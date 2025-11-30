use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as EmbassyMutex;
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::i2c::master::Config as I2cConfig;
use esp_hal::i2c::master::{Error as I2cError, I2c, Instance};
use esp_hal::time::Rate;
use esp_hal::Blocking;
use heapless::Vec;

static I2C: EmbassyMutex<CriticalSectionRawMutex, Option<I2c<Blocking>>> = EmbassyMutex::new(None);

/// 初始化 I2C
///
/// 配置 I2C 接口并设置 GPIO 引脚方向：
/// - P0 端口配置为输入模式，用于按键检测
/// - P1 端口部分配置为输出模式，用于 LCD 控制信号
///
/// # 参数
/// * `i2c` - I2C 实例
/// * `sda` - SDA 引脚
/// * `scl` - SCL 引脚
///
/// # Panics
///
/// 当 I2C 初始化失败时会 panic
pub async fn init(
    i2c: impl Instance + 'static,
    sda: impl PeripheralOutput<'static>,
    scl: impl PeripheralOutput<'static>,
) {
    // 配置 I2C 时钟频率为 400kHz，这是 OV5640 SCCB 接口要求的频率
    // 如果出现问题，可以尝试降低到100kHz进行测试
    let config = I2cConfig::default().with_frequency(Rate::from_khz(400));
    // let config = I2cConfig::default().with_frequency(Rate::from_khz(100)); // 备用的更低频率

    let i2c = I2c::new(i2c, config)
        .expect("Failed to initialize I2C")
        .with_sda(sda)
        .with_scl(scl);

    I2C.lock().await.replace(i2c);
}

/// 通过闭包访问 I2C 实例
///
/// # 参数
/// * `f` - 闭包函数，接受 I2C 实例作为参数
pub async fn with_i2c<F, R>(f: F) -> Result<R, I2cError>
where
    F: FnOnce(&mut I2c<Blocking>) -> Result<R, I2cError>,
{
    let mut guard = I2C.lock().await;
    let mut i2c_ref = guard.as_mut().unwrap();
    f(&mut i2c_ref)
}

/// I2C 写操作
///
/// # 参数
/// * `addr` - 设备地址 (7位地址格式)
/// * `reg` - 寄存器地址 (16位)
/// * `value` - 要写入的值
pub async fn write_register(addr: u8, reg: u16, value: u8) -> Result<(), I2cError> {
    // 确保使用正确的寄存器地址顺序（大端）
    let data = [((reg >> 8) as u8), (reg & 0xFF) as u8, value];
    with_i2c(|i2c| {
        i2c.write(addr, &data)
    }).await
}

/// I2C 读操作
/// 
/// # 参数
/// * `addr` - 设备地址 (7位地址格式)
/// * `reg` - 寄存器地址 (16位)
/// 
/// # 返回值
/// 读取到的值
pub async fn read_register(addr: u8, reg: u16) -> Result<u8, I2cError> {
    with_i2c(|i2c| {
        let mut value = [0u8];
        // 分两步操作：先写寄存器地址，再读取值
        i2c.write_read(addr, &[((reg >> 8) as u8), (reg & 0xFF) as u8], &mut value)?;
        Ok(value[0])
    }).await
}

/// 检测 I2C 设备是否存在
/// 
/// # 参数
/// * `addr` - 设备地址 (7位地址格式)
/// 
/// # 返回值
/// 如果设备存在返回 Ok(true)，否则返回 Ok(false)
pub async fn detect_device(addr: u8) -> Result<bool, I2cError> {
    with_i2c(|i2c| {
        // 尝试向设备发送一个空的写操作来检测设备是否存在
        match i2c.write(addr, &[]) {
            Ok(_) => Ok(true),
            Err(_e) => {
                // 任何错误都认为设备不存在或无法通信
                Ok(false)
            }
        }
    }).await
}

/// 详细扫描I2C总线并报告所有设备
/// 
/// # 返回值
/// 找到的设备地址列表
pub async fn scan_bus() -> Result<Vec<u8, 32>, I2cError> {
    let mut devices: Vec<u8, 32> = Vec::new();
    
    for addr in 0x08..0x78 {
        match detect_device(addr).await {
            Ok(true) => {
                devices.push(addr).unwrap();
            },
            Ok(false) => {}, // No device found
            Err(e) => {
                // 即使检测出错，也记录下来
                defmt::info!("Error scanning address 0x{:02x}: {:?}", addr, e);
            }
        }
    }
    
    Ok(devices)
}

/// 读取OV5640芯片ID
/// 
/// # 参数
/// * `addr` - OV5640 7位地址 (应该是0x3C)
/// 
/// # 返回值
/// 芯片ID (应该是0x5640)
pub async fn read_ov5640_chip_id(addr: u8) -> Result<u16, I2cError> {
    let mut high_byte = [0u8];
    let mut low_byte = [0u8];
    
    // 读取高字节 (寄存器0x300A)
    with_i2c(|i2c| {
        i2c.write_read(addr, &[0x30, 0x0A], &mut high_byte)
    }).await?;
    
    // 读取低字节 (寄存器0x300B)
    with_i2c(|i2c| {
        i2c.write_read(addr, &[0x30, 0x0B], &mut low_byte)
    }).await?;
    
    let chip_id = ((high_byte[0] as u16) << 8) | (low_byte[0] as u16);
    Ok(chip_id)
}