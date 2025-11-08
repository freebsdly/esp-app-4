use crate::i2c;
use embassy_time::Timer;
use esp_hal::i2c::master::Error as I2cError;
use esp_hal::i2c::master::I2c;
use esp_hal::Blocking;

/// XL9555 I2C GPIO 扩展芯片驱动
///
/// 该模块提供了对 XL9555 GPIO 扩展芯片的完整控制功能，包括：
/// - LCD 背光控制
/// - LCD 复位控制
/// - 按键输入检测
///
/// XL9555 具有 16 个 GPIO 引脚，分为两个 8 位端口：
/// - P0 端口：P0.0-P0.7 (按键连接在此端口)
/// - P1 端口：P1.0-P1.7 (LCD 控制信号连接在此端口)
///
/// # 使用方法
///
/// 1. 调用 [init] 函数初始化 XL9555
/// 2. 调用 [init_atk_md0240] 函数初始化 LCD 模块
/// 3. 调用 [set_lcd_backlight] 函数控制 LCD 背光
/// 4. 启动 [read_keys] 任务检测按键输入
pub const XL9555_ADDR: u8 = 0x20; // 7-bit I2C 地址

/// 寄存器地址定义
///
/// XL9555 芯片包含以下寄存器：
/// - 输入端口寄存器：用于读取 GPIO 引脚状态
/// - 输出端口寄存器：用于设置 GPIO 引脚输出状态
/// - 极性反转寄存器：用于设置 GPIO 引脚极性
/// - 配置寄存器：用于设置 GPIO 引脚方向（输入/输出）
///
/// 寄存器地址说明：
/// - INPUT_PORT_0: 0x00 - P0 端口输入寄存器
/// - INPUT_PORT_1: 0x01 - P1 端口输入寄存器
/// - OUTPUT_PORT_0: 0x02 - P0 端口输出寄存器
/// - OUTPUT_PORT_1: 0x03 - P1 端口输出寄存器
/// - INVERSION_PORT_0: 0x04 - P0 端口极性反转寄存器
/// - INVERSION_PORT_1: 0x05 - P1 端口极性反转寄存器
/// - CONFIG_PORT_0: 0x06 - P0 端口方向配置寄存器
/// - CONFIG_PORT_1: 0x07 - P1 端口方向配置寄存器
#[allow(unused)]
pub mod registers {
    pub const INPUT_PORT_0: u8 = 0;
    pub const INPUT_PORT_1: u8 = 1;
    pub const OUTPUT_PORT_0: u8 = 2;
    pub const OUTPUT_PORT_1: u8 = 3;
    pub const INVERSION_PORT_0: u8 = 4;
    pub const INVERSION_PORT_1: u8 = 5;
    pub const CONFIG_PORT_0: u8 = 6;
    pub const CONFIG_PORT_1: u8 = 7;
}

/// IO 位定义
///
/// 定义 XL9555 各个 IO 引脚的功能分配
/// IO 引脚分为两组：
/// - P0 端口（P0.0-P0.7）：主要用于按键输入
/// - P1 端口（P1.0-P1.7）：主要用于 LCD 控制信号输出
///
/// 引脚分配说明：
/// - LCD_BL_IO: P1.0 - LCD 背光控制（备用）
/// - SLCD_RST_IO: P1.2 - SPI LCD 复位信号
/// - SLCD_PWR_IO: P1.3 - SPI LCD 电源/背光控制
/// - KEY0_IO: P1.7 - 按键 0 输入
/// - KEY1_IO: P1.6 - 按键 1 输入
/// - KEY2_IO: P1.5 - 按键 2 输入
/// - KEY3_IO: P1.4 - 按键 3 输入
#[allow(unused)]
pub mod io_bits {
    pub const AP_INT_IO: u16 = 0x0001; // P0.0
    pub const QMA_INT_IO: u16 = 0x0002; // P0.1
    pub const SPK_EN_IO: u16 = 0x0004; // P0.2
    pub const BEEP_IO: u16 = 0x0008; // P0.3
    pub const OV_PWDN_IO: u16 = 0x0010; // P0.4
    pub const OV_RESET_IO: u16 = 0x0020; // P0.5
    pub const GBC_LED_IO: u16 = 0x0040; // P0.6
    pub const GBC_KEY_IO: u16 = 0x0080; // P0.7
    pub const LCD_BL_IO: u16 = 0x0100; // P1.0
    pub const CT_RST_IO: u16 = 0x0200; // P1.1
    pub const SLCD_RST_IO: u16 = 0x0400; // P1.2
    pub const SLCD_PWR_IO: u16 = 0x0800; // P1.3
    pub const KEY3_IO: u16 = 0x1000; // P1.4
    pub const KEY2_IO: u16 = 0x2000; // P1.5
    pub const KEY1_IO: u16 = 0x4000; // P1.6
    pub const KEY0_IO: u16 = 0x8000; // P1.7
}

/// 初始化 XL9555 芯片
///
/// 配置 I2C 接口并设置 GPIO 引脚方向：
/// - P0 端口配置为输入模式，用于按键检测
/// - P1 端口部分配置为输出模式，用于 LCD 控制信号
///
pub async fn init() -> Result<(), I2cError> {
    i2c::with_i2c(|i2c_ref| {
        // 配置XL9555 IO方向 (0表示输出，1表示输入)
        // P0全部配置为输入 (按键等)
        // P1配置为输出，但按键引脚配置为输入
        // P1.0-P1.3 为输出（LCD控制）
        // P1.4-P1.7 为输入（按键）
        // 配置 P0 端口为输入模式
        // P0 端口连接按键，需要配置为输入模式以检测按键状态
        i2c_ref.write(XL9555_ADDR, &[registers::CONFIG_PORT_0, 0xFF])?;
        // 配置 P1 端口方向
        // P1 端口混合使用，低 4 位用于 LCD 控制（输出），高 4 位用于按键（输入）
        // 0xF0 表示高 4 位为输入(1)，低 4 位为输出(0)
        i2c_ref.write(XL9555_ADDR, &[registers::CONFIG_PORT_1, 0xF0])?;

        // 初始化 P0 端口输出状态
        // 将 P0 端口输出寄存器初始化为 0
        i2c_ref
            .write(XL9555_ADDR, &[registers::OUTPUT_PORT_0, 0x00])
            .ok();
        // 初始化 P1 端口输出状态
        // 将 P1 端口输出寄存器初始化为 0
        i2c_ref.write(XL9555_ADDR, &[registers::OUTPUT_PORT_1, 0x00])
    })
    .await
}

/// 从 XL9555 的输入端口读取数据
///
/// 该函数读取 XL9555 芯片的两个输入端口（P0 和 P1）的数据。
/// P0 端口通常用于按键输入检测，P1 端口用于其他输入信号。
///
/// # 参数
/// * `i2c_ref` - I2C 接口引用
///
/// # 返回值
/// 返回包含两个端口数据的元组 (port0_data, port1_data)
pub(crate) fn read_input_ports(i2c_ref: &mut I2c<Blocking>) -> ([u8; 1], [u8; 1]) {
    // 读取 P0 端口输入状态
    // 通过读取输入端口寄存器获取 P0 端口当前的电平状态
    let mut port0_data = [0u8];
    // 读取 P1 端口输入状态
    // 通过读取输入端口寄存器获取 P1 端口当前的电平状态
    let mut port1_data = [0u8];

    i2c_ref
        .write_read(XL9555_ADDR, &[registers::INPUT_PORT_0], &mut port0_data)
        .ok();
    i2c_ref
        .write_read(XL9555_ADDR, &[registers::INPUT_PORT_1], &mut port1_data)
        .ok();

    (port0_data, port1_data)
}

// 控制 SPI LCD 电源状态
///
/// 操作 I2C 接口控制 XL9555 的 P1.3 引脚来控制 LCD 电源（背光）
/// 根据硬件设计，该引脚连接到 ATK-MD0240 模块的 PWR 引脚
/// PWR 引脚带有下拉电阻，当引脚被拉高时背光点亮，拉低或悬空时背光关闭
///
/// # 参数
/// * `i2c` - I2C 接口引用
/// * `state` - 电源状态，true 表示开启（高电平），false 表示关闭（低电平）
pub fn set_spi_lcd_power_state(i2c_ref: &mut I2c<Blocking>, state: bool) -> Result<(), I2cError> {
    // 读取当前端口1输出状态
    let mut port1_data = [0u8];
    i2c_ref.write_read(XL9555_ADDR, &[registers::OUTPUT_PORT_1], &mut port1_data)?;
    // 根据状态设置 SPI LCD 电源引脚 (P1.3)
    let new_port1_data = if state {
        port1_data[0] | (io_bits::SLCD_PWR_IO >> 8) as u8 // 设置P1.3为高电平
    } else {
        port1_data[0] & !((io_bits::SLCD_PWR_IO >> 8) as u8) // 设置P1.3为低电平
    };

    // 写回端口1输出
    i2c_ref.write(XL9555_ADDR, &[registers::OUTPUT_PORT_1, new_port1_data])
}

// 控制 SPI LCD 复位状态
///
/// 操作 I2C 接口控制 XL9555 的 P1.2 引脚来控制 LCD 复位信号
///
/// # 参数
/// * `i2c` - I2C 接口引用
/// * `state` - 复位状态，true 表示复位释放（高电平），false 表示复位（低电平）
pub fn set_spi_lcd_reset_state(i2c_ref: &mut I2c<Blocking>, state: bool) -> Result<(), I2cError> {
    // 读取当前端口1输出状态
    let mut port1_data = [0u8];
    i2c_ref.write_read(XL9555_ADDR, &[registers::OUTPUT_PORT_1], &mut port1_data)?;
    // 根据状态设置 SPI LCD 复位引脚 (P1.2)
    let new_port1_data = if state {
        port1_data[0] | (io_bits::SLCD_RST_IO >> 8) as u8 // 设置P1.2为高电平
    } else {
        port1_data[0] & !((io_bits::SLCD_RST_IO >> 8) as u8) // 设置P1.2为低电平
    };

    // 写回端口1输出
    i2c_ref.write(XL9555_ADDR, &[registers::OUTPUT_PORT_1, new_port1_data])
}

// 添加公共函数用于外部调用
pub async fn spi_lcd_reset(state: bool) -> Result<(), I2cError> {
    i2c::with_i2c(|i2c_ref| set_spi_lcd_reset_state(i2c_ref, state)).await
}

/// 公共接口函数：控制 LCD 背光开关
///
/// 通过该函数可以外部调用设置 LCD 背光的开关状态
/// 控制的是 XL9555 的 P1.3 引脚，该引脚连接到 ATK-MD0240 模块的 PWR 引脚
///
/// # 参数
/// * `state` - 背光状态，true 表示开启背光，false 表示关闭背光
pub async fn set_lcd_backlight(state: bool) -> Result<(), I2cError> {
    i2c::with_i2c(|i2c_ref| set_spi_lcd_power_state(i2c_ref, state)).await
}

/// 初始化ATK-MD0240模块
/// 执行硬件复位序列：RST引脚拉低至少10微秒，然后拉高并延时120毫秒等待复位完成
pub async fn init_atk_md0240() -> Result<(), I2cError> {
    // 拉低RST引脚至少10微秒
    spi_lcd_reset(false).await?;
    Timer::after_micros(10).await;

    // 拉高RST引脚
    spi_lcd_reset(true).await?;

    // 延时120毫秒等待复位完成
    Timer::after_millis(120).await;
    Ok(())
}
