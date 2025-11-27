//! QMA6100P 三轴加速度传感器驱动
//!
//! QMA6100P 是一款超低功耗的三轴加速度传感器，具有以下特点：
//! - 超低功耗设计：工作电流仅6-38μA
//! - 多种量程可选：±2g/±4g/±8g/±16g/±32g
//! - 高分辨率：14位ADC，12位有效精度
//! - 支持I2C和SPI接口
//! - 内置FIFO缓冲区（64级）
//! - 多种中断功能：运动检测、静止检测、敲击检测等

use crate::i2c;
use esp_hal::i2c::master::Error as I2cError;

/// QMA6100P I2C 地址 (AD0=0)
pub const QMA6100P_ADDR_AD0_LOW: u8 = 0x12;
/// QMA6100P I2C 地址 (AD0=1)
pub const QMA6100P_ADDR_AD0_HIGH: u8 = 0x13;

/// QMA6100P 设备ID
pub const QMA6100P_DEVICE_ID: u8 = 0x55;

/// QMA6100P 寄存器地址定义
#[allow(unused)]
pub mod registers {
    /// 芯片ID寄存器
    pub const CHIP_ID: u8 = 0x00;

    /// X轴加速度数据低字节
    pub const ACC_X_LSB: u8 = 0x01;
    /// X轴加速度数据高字节
    pub const ACC_X_MSB: u8 = 0x02;
    /// Y轴加速度数据低字节
    pub const ACC_Y_LSB: u8 = 0x03;
    /// Y轴加速度数据高字节
    pub const ACC_Y_MSB: u8 = 0x04;
    /// Z轴加速度数据低字节
    pub const ACC_Z_LSB: u8 = 0x05;
    /// Z轴加速度数据高字节
    pub const ACC_Z_MSB: u8 = 0x06;

    /// 系统控制寄存器1 (模式控制)
    pub const CTRL_REG1: u8 = 0x11;

    /// 重置寄存器
    pub const REG_RESET: u8 = 0x7E;

    /// 量程设置寄存器
    pub const REG_RANGE: u8 = 0x0F;

    /// 带宽和输出数据率寄存器
    pub const REG_BW_ODR: u8 = 0x10;

    /// 电源管理寄存器
    pub const REG_POWER_MANAGE: u8 = 0x12;

    /// 中断使能寄存器1
    pub const INT_EN_1: u8 = 0x19;
    /// 中断使能寄存器2
    pub const INT_EN_2: u8 = 0x1A;
    /// 中断映射寄存器1
    pub const INT_MAP_1: u8 = 0x21;

    /// FIFO控制寄存器
    pub const FIFO_CTRL: u8 = 0x3D;
    /// FIFO数据寄存器
    pub const FIFO_DATA: u8 = 0x3F;

    /// 厂家推荐初始化寄存器
    pub const REG_4A: u8 = 0x4A;
    pub const REG_56: u8 = 0x56;
    pub const REG_5F: u8 = 0x5F;
}

/// 量程设置
#[derive(Debug, Clone, Copy)]
pub enum Range {
    /// ±2g量程
    G2 = 0b0001,
    /// ±4g量程
    G4 = 0b0010,
    /// ±8g量程
    G8 = 0b0100,
    /// ±16g量程
    G16 = 0b1000,
    /// ±32g量程
    G32 = 0b1111,
}

impl Range {
    /// 获取量程对应的LSB/g值
    pub fn lsb_per_g(&self) -> f32 {
        match self {
            Range::G2 => 4096.0, // 2g: 4096 LSB/g
            Range::G4 => 2048.0, // 4g: 2048 LSB/g
            Range::G8 => 1024.0, // 8g: 1024 LSB/g
            Range::G16 => 512.0, // 16g: 512 LSB/g
            Range::G32 => 256.0, // 32g: 256 LSB/g
        }
    }

    /// 获取量程值（单位：g）
    pub fn range_g(&self) -> f32 {
        match self {
            Range::G2 => 2.0,
            Range::G4 => 4.0,
            Range::G8 => 8.0,
            Range::G16 => 16.0,
            Range::G32 => 32.0,
        }
    }
}

/// 工作模式
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    /// 待机模式（低功耗）
    Standby = 0,
    /// 激活模式（正常工作）
    Active = 1,
}

/// 带宽设置
#[allow(unused)]
pub const QMA6100P_BW_100: u8 = 0x0A;

/// 时钟设置
#[allow(unused)]
pub const QMA6100P_MCLK_51_2K: u8 = 0x04;

/// 三轴加速度数据
#[derive(Debug, Default)]
pub struct AccelerationData {
    /// X轴加速度值（单位：g）
    pub x: f32,
    /// Y轴加速度值（单位：g）
    pub y: f32,
    /// Z轴加速度值（单位：g）
    pub z: f32,
}

/// 俯仰角和横滚角数据
#[derive(Debug, Default)]
pub struct AngleData {
    /// 俯仰角（Pitch，绕X轴旋转）单位：度
    pub pitch: f32,
    /// 横滚角（Roll，绕Y轴旋转）单位：度
    pub roll: f32,
}

/// 原始三轴加速度数据
#[derive(Debug, Default)]
pub struct RawAccelerationData {
    /// X轴原始数据
    pub x: i16,
    /// Y轴原始数据
    pub y: i16,
    /// Z轴原始数据
    pub z: i16,
}

/// QMA6100P传感器驱动
pub struct Qma6100p {
    /// I2C地址
    addr: u8,
    /// 当前量程
    range: Range,
    /// 滤波缓冲区
    filter_buffer: FilterBuffer,
}

/// 滤波缓冲区
struct FilterBuffer {
    /// 缓冲区数据 [FILTER_SIZE][3] - 5组三轴数据
    buffer: [[f32; 3]; 5],
    /// 当前索引
    index: usize,
}

impl FilterBuffer {
    /// 创建新的滤波缓冲区
    const fn new() -> Self {
        Self {
            buffer: [[0.0; 3]; 5],
            index: 0,
        }
    }

    /// 更新滤波缓冲区并计算移动平均值
    fn update_and_filter(&mut self, new_data: &[f32; 3]) -> [f32; 3] {
        // 更新滤波缓冲区
        self.buffer[self.index] = *new_data;
        self.index = (self.index + 1) % 5;

        // 计算移动平均值
        let mut filtered_data = [0.0; 3];
        for axis in 0..3 {
            let mut sum = 0.0;
            for i in 0..5 {
                sum += self.buffer[i][axis];
            }
            filtered_data[axis] = sum / 5.0;
        }

        filtered_data
    }
}

impl Qma6100p {
    /// 创建新的QMA6100P传感器实例
    ///
    /// # 参数
    /// * `addr` - I2C地址，可以是 [QMA6100P_ADDR_AD0_LOW] 或 [QMA6100P_ADDR_AD0_HIGH]
    pub fn new(addr: u8) -> Self {
        Qma6100p {
            addr,
            range: Range::G8, // 默认使用±8g量程
            filter_buffer: FilterBuffer::new(),
        }
    }

    /// 初始化传感器
    ///
    /// # 返回值
    /// * `Ok(())` - 成功初始化
    /// * `Err(I2cError)` - 初始化过程中发生I2C错误
    pub async fn init(&mut self) -> Result<(), I2cError> {
        i2c::with_i2c(|i2c_ref| {
            // 读取芯片ID验证连接
            let mut chip_id = [0u8];
            i2c_ref.write_read(self.addr, &[registers::CHIP_ID], &mut chip_id)?;

            defmt::info!("QMA6100P chip ID: 0x{:x}", chip_id[0]);

            if chip_id[0] != QMA6100P_DEVICE_ID {
                defmt::error!(
                    "QMA6100P device ID mismatch, expected: 0x{:x}, got: 0x{:x}",
                    QMA6100P_DEVICE_ID,
                    chip_id[0]
                );
                // 这里我们继续初始化过程，但在实际应用中可能需要返回错误
            }

            Ok(())
        })
        .await?;

        // 执行软件复位
        self.software_reset().await?;

        // 厂家推荐初始化序列
        self.vendor_recommended_sequence().await?;

        // 配置传感器参数
        self.configure_sensor().await?;

        Ok(())
    }

    /// 软件复位
    async fn software_reset(&mut self) -> Result<(), I2cError> {
        i2c::with_i2c(|i2c_ref| {
            // 发送两次复位命令
            i2c_ref.write(self.addr, &[registers::REG_RESET, 0xB6])?;
            i2c_ref.write(self.addr, &[registers::REG_RESET, 0xB6])?;
            Ok(())
        })
        .await?;

        // 等待5ms
        embassy_time::Timer::after_millis(5).await;

        i2c::with_i2c(|i2c_ref| {
            // 清除复位
            i2c_ref.write(self.addr, &[registers::REG_RESET, 0x00])?;
            Ok(())
        })
        .await?;

        // 等待10ms
        embassy_time::Timer::after_millis(10).await;

        Ok(())
    }

    /// 厂家推荐初始化序列
    async fn vendor_recommended_sequence(&mut self) -> Result<(), I2cError> {
        i2c::with_i2c(|i2c_ref| {
            i2c_ref.write(self.addr, &[registers::CTRL_REG1, 0x80])?;
            i2c_ref.write(self.addr, &[registers::CTRL_REG1, 0x84])?;
            i2c_ref.write(self.addr, &[registers::REG_4A, 0x20])?;
            i2c_ref.write(self.addr, &[registers::REG_56, 0x01])?;
            i2c_ref.write(self.addr, &[registers::REG_5F, 0x80])?;
            Ok(())
        })
        .await?;

        // 等待2ms
        embassy_time::Timer::after_millis(2).await;

        i2c::with_i2c(|i2c_ref| {
            i2c_ref.write(self.addr, &[registers::REG_5F, 0x00])?;
            Ok(())
        })
        .await?;

        // 等待10ms
        embassy_time::Timer::after_millis(10).await;

        Ok(())
    }

    /// 配置传感器参数
    async fn configure_sensor(&mut self) -> Result<(), I2cError> {
        i2c::with_i2c(|i2c_ref| {
            // 设置量程为±8g
            i2c_ref.write(self.addr, &[registers::REG_RANGE, Range::G8 as u8])?;

            // 设置带宽
            i2c_ref.write(self.addr, &[registers::REG_BW_ODR, QMA6100P_BW_100])?;

            // 设置电源管理
            i2c_ref.write(
                self.addr,
                &[registers::REG_POWER_MANAGE, QMA6100P_MCLK_51_2K | 0x80],
            )?;

            Ok(())
        })
        .await?;

        Ok(())
    }

    /// 设置传感器量程
    ///
    /// # 参数
    /// * `range` - 量程设置
    ///
    /// # 返回值
    /// * `Ok(())` - 成功设置量程
    /// * `Err(I2cError)` - 设置过程中发生I2C错误
    pub async fn set_range(&mut self, range: Range) -> Result<(), I2cError> {
        i2c::with_i2c(|i2c_ref| {
            i2c_ref.write(self.addr, &[registers::REG_RANGE, range as u8])?;
            self.range = range;
            Ok(())
        })
        .await
    }

    /// 设置工作模式
    ///
    /// # 参数
    /// * `mode` - 工作模式
    ///
    /// # 返回值
    /// * `Ok(())` - 成功设置模式
    /// * `Err(I2cError)` - 设置过程中发生I2C错误
    pub async fn set_mode(&mut self, mode: Mode) -> Result<(), I2cError> {
        i2c::with_i2c(|i2c_ref| {
            // 读取当前CTRL_REG1寄存器值
            let mut ctrl_reg1 = [0u8];
            i2c_ref.write_read(self.addr, &[registers::CTRL_REG1], &mut ctrl_reg1)?;

            // 根据模式设置第7位
            let new_val = match mode {
                Mode::Standby => ctrl_reg1[0] & !(1 << 7), // 清除第7位
                Mode::Active => ctrl_reg1[0] | (1 << 7),   // 设置第7位
            };

            // 写入新值
            i2c_ref.write(self.addr, &[registers::CTRL_REG1, new_val])?;

            Ok(())
        })
        .await
    }

    /// 读取三轴原始加速度数据
    ///
    /// # 返回值
    /// * `Ok(RawAccelerationData)` - 成功读取原始加速度数据
    /// * `Err(I2cError)` - 读取过程中发生I2C错误
    pub async fn read_raw_acceleration(&mut self) -> Result<RawAccelerationData, I2cError> {
        i2c::with_i2c(|i2c_ref| {
            // 读取6字节数据寄存器（0x01-0x06）
            let mut databuf = [0u8; 6];
            i2c_ref.write_read(self.addr, &[registers::ACC_X_LSB], &mut databuf)?;

            // 组合14位数据（低6位+高8位）
            let x_raw = ((databuf[1] as i16) << 8 | databuf[0] as i16) >> 2;
            let y_raw = ((databuf[3] as i16) << 8 | databuf[2] as i16) >> 2;
            let z_raw = ((databuf[5] as i16) << 8 | databuf[4] as i16) >> 2;

            Ok(RawAccelerationData {
                x: x_raw,
                y: y_raw,
                z: z_raw,
            })
        })
        .await
    }

    /// 读取三轴加速度数据（g单位）
    ///
    /// # 返回值
    /// * `Ok(AccelerationData)` - 成功读取加速度数据
    /// * `Err(I2cError)` - 读取过程中发生I2C错误
    pub async fn read_acceleration(&mut self) -> Result<AccelerationData, I2cError> {
        // 读取原始数据
        let raw_data = self.read_raw_acceleration().await?;

        // 转换为g单位
        // 根据提供的C代码，使用固定灵敏度1024 LSB/g（对应±8g量程）
        let sensitivity = 1024.0; // LSB/g
        let g = 9.80665; // 标准重力加速度 m/s²

        let accel_data = AccelerationData {
            x: (raw_data.x as f32 * g) / sensitivity,
            y: (raw_data.y as f32 * g) / sensitivity,
            z: (raw_data.z as f32 * g) / sensitivity,
        };

        Ok(accel_data)
    }

    /// 从三轴加速度数据计算俯仰角和横滚角
    ///
    /// # 参数
    /// * `acc_data` - 三轴加速度数据
    ///
    /// # 返回值
    /// * `AngleData` - 包含俯仰角和横滚角的数据结构
    pub fn calculate_angles(&self, acc_data: &AccelerationData) -> AngleData {
        use micromath::F32Ext;

        // 俯仰角 Pitch（绕X轴旋转）
        let pitch = (acc_data.y).atan2((acc_data.x * acc_data.x + acc_data.z * acc_data.z).sqrt())
            * 180.0
            / core::f32::consts::PI;

        // 横滚角 Roll（绕Y轴旋转）
        let roll = (-acc_data.x).atan2(acc_data.z) * 180.0 / core::f32::consts::PI;

        AngleData { pitch, roll }
    }

    /// 读取三轴加速度数据并计算角度
    ///
    /// # 返回值
    /// * `Ok((AccelerationData, AngleData))` - 成功读取加速度数据和计算角度
    /// * `Err(I2cError)` - 读取过程中发生I2C错误
    pub async fn read_acceleration_with_angles(
        &mut self,
    ) -> Result<(AccelerationData, AngleData), I2cError> {
        let accel_data = self.read_acceleration().await?;
        let angle_data = self.calculate_angles(&accel_data);
        Ok((accel_data, angle_data))
    }

    /// 读取三轴加速度数据（g单位，带滤波）
    ///
    /// # 返回值
    /// * `Ok(AccelerationData)` - 成功读取滤波后的加速度数据
    /// * `Err(I2cError)` - 读取过程中发生I2C错误
    pub async fn read_filtered_acceleration(&mut self) -> Result<AccelerationData, I2cError> {
        // 读取原始数据
        let raw_data = self.read_raw_acceleration().await?;

        // 转换为g单位
        let sensitivity = 1024.0; // LSB/g
        let g = 9.80665; // 标准重力加速度 m/s²

        let new_data = [
            (raw_data.x as f32 * g) / sensitivity,
            (raw_data.y as f32 * g) / sensitivity,
            (raw_data.z as f32 * g) / sensitivity,
        ];

        // 应用移动平均滤波
        let filtered_data = self.filter_buffer.update_and_filter(&new_data);

        let accel_data = AccelerationData {
            x: filtered_data[0],
            y: filtered_data[1],
            z: filtered_data[2],
        };

        Ok(accel_data)
    }

    /// 读取三轴加速度数据并计算角度（带滤波）
    ///
    /// # 返回值
    /// * `Ok((AccelerationData, AngleData))` - 成功读取滤波后的加速度数据和计算角度
    /// * `Err(I2cError)` - 读取过程中发生I2C错误
    pub async fn read_filtered_acceleration_with_angles(
        &mut self,
    ) -> Result<(AccelerationData, AngleData), I2cError> {
        let accel_data = self.read_filtered_acceleration().await?;
        let angle_data = self.calculate_angles(&accel_data);
        Ok((accel_data, angle_data))
    }
}

impl Default for Qma6100p {
    fn default() -> Self {
        Self::new(QMA6100P_ADDR_AD0_LOW)
    }
}

/// QMA6100P传感器任务，定期读取并打印传感器数据
#[embassy_executor::task]
pub async fn qma6100p_task(mut sensor: Qma6100p) {
    // 确保传感器处于激活模式
    let _ = sensor.set_mode(Mode::Active).await;

    loop {
        match sensor.read_filtered_acceleration_with_angles().await {
            Ok((accel, angles)) => {
                defmt::info!(
                    "QMA6100P Accel: X={:03}g, Y={:03}g, Z={:03}g",
                    accel.x,
                    accel.y,
                    accel.z
                );
                defmt::info!(
                    "QMA6100P Angles: Pitch={:02}°, Roll={:02}°",
                    angles.pitch,
                    angles.roll
                );
            }
            Err(e) => {
                defmt::info!("Failed to read QMA6100P sensor: {:?}", e);
            }
        }

        // 等待100毫秒后再次读取
        embassy_time::Timer::after_millis(100).await;
    }
}
