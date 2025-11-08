use core::cell::RefCell;
use critical_section::Mutex;
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::i2c::master::Config as I2cConfig;
use esp_hal::i2c::master::{I2c, Instance, Error as I2cError};
use esp_hal::Blocking;

static I2C: Mutex<RefCell<Option<I2c<Blocking>>>> = Mutex::new(RefCell::new(None));

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
    let i2c = I2c::new(i2c, I2cConfig::default())
        .expect("Failed to initialize I2C")
        .with_sda(sda)
        .with_scl(scl);

    critical_section::with(|cs| {
        I2C.borrow_ref_mut(cs).replace(i2c);
    });
}

/// 通过闭包访问 I2C 实例
///
/// # 参数
/// * `f` - 闭包函数，接受 I2C 实例作为参数
pub fn with_i2c<F, R>(f: F) -> Result<R, I2cError>
where
    F: FnOnce(&mut I2c<Blocking>) -> Result<R, I2cError>,
{
    critical_section::with(|cs| {
        let mut i2c_ref = I2C.borrow_ref_mut(cs);
        let mut i2c = i2c_ref.as_mut().unwrap();
        f(&mut i2c)
    })
}

/// 通过闭包访问 I2C 实例（无返回值版本）
///
/// # 参数
/// * `f` - 闭包函数，接受 I2C 实例作为参数
#[allow(unused)]
pub fn with_i2c_mut<F>(f: F)
where
    F: FnOnce(&mut I2c<Blocking>),
{
    critical_section::with(|cs| {
        let mut i2c_ref = I2C.borrow_ref_mut(cs);
        let mut i2c = i2c_ref.as_mut().unwrap();
        f(&mut i2c);
    })
}
