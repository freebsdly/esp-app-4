//! AP3216C 传感器驱动
//!
//! AP3216C是一款集成了环境光传感器(ALS)、接近传感器(PS)和红外LED(IR LED)的三合一数字传感器。
//! 该模块提供了对AP3216C传感器的基本控制功能，包括初始化、读取传感器数据等。

use crate::i2c;
use esp_hal::i2c::master::Error as I2cError;

/// AP3216C I2C地址
pub const AP3216C_ADDR: u8 = 0x1E;

/// AP3216C寄存器地址
#[allow(unused)]
pub mod registers {
    /// 系统配置寄存器
    pub const SYSTEM_CONFIG: u8 = 0x00;
    /// 中断清除寄存器
    pub const INT_CLEAR: u8 = 0x01;
    /// 模式控制寄存器
    pub const MODE_CONFIG: u8 = 0x02;
    /// ALS时间控制寄存器
    pub const ALS_TIME: u8 = 0x03;
    /// PS LED控制寄存器
    pub const PS_LED: u8 = 0x04;
    /// PS中断阈值寄存器
    pub const PS_INT_HT: u8 = 0x05;
    /// IR数据低字节寄存器
    pub const IR_DATA_LOW: u8 = 0x0A;
    /// IR数据高字节寄存器
    pub const IR_DATA_HIGH: u8 = 0x0B;
    /// ALS数据低字节寄存器
    pub const ALS_DATA_LOW: u8 = 0x0C;
    /// ALS数据高字节寄存器
    pub const ALS_DATA_HIGH: u8 = 0x0D;
    /// PS数据低字节寄存器
    pub const PS_DATA_LOW: u8 = 0x0E;
    /// PS数据高字节寄存器
    pub const PS_DATA_HIGH: u8 = 0x0F;
    /// 芯片ID寄存器
    pub const PART_ID: u8 = 0x12;
}

/// AP3216C工作模式
#[derive(Debug, Clone, Copy)]
pub enum Ap3216cMode {
    /// 断电模式
    PowerDown = 0,
    /// 仅ALS模式
    AlsOnly = 1,
    /// 仅PS+IR模式
    PsIrOnly = 2,
    /// ALS+PS+IR模式
    AlsPsIr = 3,
    /// 软件复位
    SwReset = 4,
    /// ALS单次测量模式
    AlsOnce = 5,
    /// PS+IR单次测量模式
    PsIrOnce = 6,
    /// ALS+PS+IR单次测量模式
    AlsPsIrOnce = 7,
}

/// AP3216C传感器数据
#[derive(Debug, Default)]
pub struct SensorData {
    /// 环境光强度 (lux)
    pub als_data: u16,
    /// 接近传感器数据
    pub ps_data: u16,
    /// 红外传感器数据
    pub ir_data: u16,
}

/// AP3216C传感器驱动
pub struct Ap3216c {}

impl Ap3216c {
    /// 创建新的AP3216C传感器实例
    pub fn new() -> Self {
        Ap3216c {}
    }

    /// 初始化AP3216C传感器
    ///
    /// # 返回值
    /// * `Ok(())` - 成功初始化
    /// * `Err(I2cError)` - 初始化过程中发生I2C错误
    pub async fn init(&mut self) -> Result<(), I2cError> {
        i2c::with_i2c(|i2c_ref| {
            // 1. 软件复位
            i2c_ref.write(
                AP3216C_ADDR,
                &[registers::SYSTEM_CONFIG, Ap3216cMode::SwReset as u8],
            )?;

            // 等待复位完成
            // 注意：这里我们需要在闭包外部等待，因为闭包内无法使用异步函数
            Ok(())
        })
        .await?;

        // 等待复位完成
        embassy_time::Timer::after_millis(10).await;

        i2c::with_i2c(|i2c_ref| {
            // 2. 读取芯片ID验证连接
            let mut id_data = [0u8];
            i2c_ref.write_read(AP3216C_ADDR, &[registers::PART_ID], &mut id_data)?;

            // 3. 配置传感器模式为ALS+PS+IR
            i2c_ref.write(
                AP3216C_ADDR,
                &[registers::SYSTEM_CONFIG, Ap3216cMode::AlsPsIr as u8],
            )?;

            Ok(())
        })
        .await?;

        // 4. 等待传感器稳定
        embassy_time::Timer::after_millis(15).await;

        Ok(())
    }

    /// 读取传感器数据
    ///
    /// # 返回值
    /// * `Ok(SensorData)` - 成功读取传感器数据
    /// * `Err(I2cError)` - 读取过程中发生I2C错误
    pub async fn read_sensor_data(&mut self) -> Result<SensorData, I2cError> {
        i2c::with_i2c(|i2c_ref| {
            // 读取ALS数据
            let mut als_low = [0u8];
            let mut als_high = [0u8];
            i2c_ref.write_read(AP3216C_ADDR, &[registers::ALS_DATA_LOW], &mut als_low)?;
            i2c_ref.write_read(AP3216C_ADDR, &[registers::ALS_DATA_HIGH], &mut als_high)?;

            let als_data = ((als_high[0] as u16) << 8) | (als_low[0] as u16);

            // 读取PS数据
            let mut ps_low = [0u8];
            let mut ps_high = [0u8];
            i2c_ref.write_read(AP3216C_ADDR, &[registers::PS_DATA_LOW], &mut ps_low)?;
            i2c_ref.write_read(AP3216C_ADDR, &[registers::PS_DATA_HIGH], &mut ps_high)?;

            // PS数据需要特殊处理，因为只有低位的4位有效
            let ps_data = (((ps_high[0] & 0x3F) as u16) << 4) | ((ps_low[0] & 0xF0) as u16 >> 4);
            
            // 读取IR数据
            let mut ir_low = [0u8];
            let mut ir_high = [0u8];
            i2c_ref.write_read(AP3216C_ADDR, &[registers::IR_DATA_LOW], &mut ir_low)?;
            i2c_ref.write_read(AP3216C_ADDR, &[registers::IR_DATA_HIGH], &mut ir_high)?;
            
            // IR数据需要特殊处理，低字节的低2位和高字节组成完整数据
            let ir_data = (((ir_high[0] as u16) << 2) | ((ir_low[0] & 0x03) as u16)) as u16;

            Ok(SensorData { als_data, ps_data, ir_data })
        })
        .await
    }
}

impl Default for Ap3216c {
    fn default() -> Self {
        Self::new()
    }
}

/// AP3216C传感器任务，定期读取并打印传感器数据
#[embassy_executor::task]
pub async fn ap3216c_task(mut sensor: Ap3216c) {
    loop {
        match sensor.read_sensor_data().await {
            Ok(data) => {
                defmt::info!("AP3216C ALS: {} lux, PS: {}, IR: {}", data.als_data, data.ps_data, data.ir_data);
            }
            Err(e) => {
                defmt::info!("Failed to read AP3216C sensor: {:?}", e);
            }
        }

        // 等待1秒后再次读取
        embassy_time::Timer::after_secs(1).await;
    }
}
