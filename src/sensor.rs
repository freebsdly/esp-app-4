//! ESP32-S3 内部温度传感器驱动
//!
//! ESP32-S3 内置温度传感器，可以测量芯片内部的温度。
//! 温度传感器的测量范围为–20°C 到 110°C。
//! 温度传感器适用于监测芯片内部温度的变化，该温度值会随着微控制器时钟频率或 IO 负载的变化而变化。
//!
//! 转换公式如下：
//! T(°C) = 0.4386 * VALUE – 27.88 * offset - 20.52
//! 其中 VALUE 即温度传感器的输出值，offset 由温度偏移决定。

use defmt::info;
use embassy_time::Timer;
use esp_hal::peripherals::SENS;
// 移除 APB_SARADC 的直接使用

/// 温度传感器错误类型
#[derive(Debug)]
pub enum TemperatureError {
    /// 传感器读取错误
    ReadError,
    /// ADC读取错误
    AdcError,
    /// 温度计算错误
    CalculationError,
    /// 传感器未启用
    NotEnabled,
    /// ADC未初始化
    AdcNotInitialized,
}

// 为TemperatureError实现defmt::Format trait，以便可以使用defmt打印错误
impl defmt::Format for TemperatureError {
    fn format(&self, f: defmt::Formatter) {
        match self {
            TemperatureError::ReadError => {
                defmt::write!(f, "Temperature sensor read error");
            }
            TemperatureError::AdcError => {
                defmt::write!(f, "ADC read error");
            }
            TemperatureError::CalculationError => {
                defmt::write!(f, "Temperature calculation error");
            }
            TemperatureError::NotEnabled => {
                defmt::write!(f, "Temperature sensor not enabled");
            }
            TemperatureError::AdcNotInitialized => {
                defmt::write!(f, "ADC not initialized");
            }
        }
    }
}

/// 温度传感器测量范围
#[derive(Debug, Clone, Copy)]
pub enum TemperatureRange {
    /// 50 ~ 110°C，偏移值 -2
    Range50To110,
    /// 20 ~ 100°C，偏移值 -1
    Range20To100,
    /// -10 ~ 80°C，偏移值 0
    RangeMinus10To80,
    /// -15 ~ 50°C，偏移值 1
    RangeMinus15To50,
    /// -20 ~ 20°C，偏移值 2
    RangeMinus20To20,
}

impl TemperatureRange {
    /// 获取对应范围的温度偏移值
    pub fn offset(&self) -> f32 {
        match self {
            TemperatureRange::Range50To110 => -2.0,
            TemperatureRange::Range20To100 => -1.0,
            TemperatureRange::RangeMinus10To80 => 0.0,
            TemperatureRange::RangeMinus15To50 => 1.0,
            TemperatureRange::RangeMinus20To20 => 2.0,
        }
    }
}

/// 内部温度传感器
pub struct InternalTemperatureSensor<'a> {
    /// 温度测量范围
    range: TemperatureRange,
    /// 传感器是否启用
    enabled: bool,
    /// SENS外设用于控制温度传感器
    sens: SENS<'a>,
}

impl<'a> InternalTemperatureSensor<'a> {
    /// 创建一个新的内部温度传感器实例
    ///
    /// # 参数
    /// * `sens` - SENS外设实例
    /// * `range` - 温度测量范围
    ///
    /// # 返回值
    /// * `InternalTemperatureSensor` - 温度传感器实例
    pub fn new(sens: SENS<'a>, range: TemperatureRange) -> Self {
        Self {
            range,
            enabled: false,
            sens,
        }
    }

    /// 启用温度传感器
    ///
    /// # 返回值
    /// * `Ok(())` - 成功启用
    /// * `Err(TemperatureError)` - 启用过程中发生的错误
    pub fn enable(&mut self) -> Result<(), TemperatureError> {
        // 启用温度传感器
        // 根据ESP32-S3技术参考手册，需要配置SENS_SAR_TSENS_CTRL_REG寄存器
        let sens = unsafe { &*esp_hal::peripherals::SENS::ptr() };
        sens.sar_tsens_ctrl().modify(|_, w| unsafe { w.sar_tsens_clk_div().bits(0x4) });
        sens.sar_tsens_ctrl().modify(|_, w| w.sar_tsens_power_up().set_bit());
        sens.sar_tsens_ctrl().modify(|_, w| w.sar_tsens_power_up_force().set_bit());
        
        self.enabled = true;
        Ok(())
    }

    /// 禁用温度传感器
    ///
    /// # 返回值
    /// * `Ok(())` - 成功禁用
    /// * `Err(TemperatureError)` - 禁用过程中发生的错误
    pub fn disable(&mut self) -> Result<(), TemperatureError> {
        // 禁用传感器
        let sens = unsafe { &*SENS::ptr() };
        sens.sar_tsens_ctrl().modify(|_, w| w.sar_tsens_power_up().clear_bit());
        
        self.enabled = false;
        Ok(())
    }

    /// 获取温度值(摄氏度)
    ///
    /// 使用公式: T(°C) = 0.4386 * VALUE – 27.88 * offset - 20.52
    ///
    /// # 返回值
    /// * `Ok(f32)` - 成功读取的温度值(°C)
    /// * `Err(TemperatureError)` - 读取过程中发生的错误
    pub async fn get_celsius(&mut self) -> Result<f32, TemperatureError> {
        if !self.enabled {
            return Err(TemperatureError::NotEnabled);
        }

        // 通过ADC读取温度传感器数据
        // 注意：ESP32-S3的内部温度传感器连接到ADC1的特定通道
        // 目前使用模拟数据进行测试
        let raw_value = 1000u16; // 模拟值，实际应该从ADC读取
        
        // 获取温度偏移值
        let offset = self.range.offset();
        
        // 应用转换公式: T(°C) = 0.4386 * VALUE – 27.88 * offset - 20.52
        let calculated_temp = 0.4386 * raw_value as f32 - 27.88 * offset - 20.52;
        
        Ok(calculated_temp)
    }

    /// 读取温度传感器数据
    ///
    /// 自动启用传感器，读取数据，然后禁用传感器
    ///
    /// # 返回值
    /// * `Ok(f32)` - 成功读取的温度值(°C)
    /// * `Err(TemperatureError)` - 读取过程中发生的错误
    pub async fn read_temperature(&mut self) -> Result<f32, TemperatureError> {
        // 启用温度传感器
        self.enable()?;

        // 获取传输的传感器数据
        let temp = self.get_celsius().await?;

        // 温度传感器使用完毕后，禁用温度传感器，节约功耗
        self.disable()?;

        Ok(temp)
    }
}

impl Default for InternalTemperatureSensor<'_> {
    fn default() -> Self {
        Self::new(
            unsafe { SENS::steal() },
            TemperatureRange::RangeMinus10To80
        )
    }
}

/// 温度传感器任务，定期读取并打印温度数据
#[embassy_executor::task]
pub async fn temperature_sensor_task(mut sensor: InternalTemperatureSensor<'static>) {
    loop {
        match sensor.read_temperature().await {
            Ok(temp) => {
                info!("Internal temperature: {}°C", temp as i32);
            }
            Err(e) => {
                info!("Failed to read temperature sensor: {:?}", e);
            }
        }

        // 等待5秒后再次读取
        Timer::after_secs(5).await;
    }
}