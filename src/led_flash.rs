//! LED Flash Module for OV5640 Camera
//!
//! This module provides functionality for controlling the LED flash feature
//! of the OV5640 camera sensor through the XL9555 GPIO expander.
//!
//! Based on the information from the datasheet:
//! - STROBE pin (E1) for control
//! - Supports LED flash and xenon flash modes
//! - Maximum current: 200mA (requires external drive circuit)
//! - Response speed: <100μs fast response

use defmt::info;
use embassy_time::{Duration, Instant, Timer};
use micromath::F32Ext;

/// STROBE控制模式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StrobeMode {
    /// 禁用STROBE
    Disable = 0x00,
    /// LED闪光模式
    LedMode = 0x01,
    /// 氙气灯模式
    XenonMode = 0x02,
    /// 自动模式
    Auto = 0x04,
}

/// 为StrobeMode实现defmt::Format trait，使其可以在info!宏中使用
impl defmt::Format for StrobeMode {
    fn format(&self, f: defmt::Formatter) {
        match self {
            StrobeMode::Disable => defmt::write!(f, "Disable"),
            StrobeMode::LedMode => defmt::write!(f, "LedMode"),
            StrobeMode::XenonMode => defmt::write!(f, "XenonMode"),
            StrobeMode::Auto => defmt::write!(f, "Auto"),
        }
    }
}

/// OV5640 LED模式定义
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LEDMode {
    /// 闪光灯模式（拍照时瞬间高亮）
    FlashLED,
    /// 手电筒模式（持续照明）
    TorchLED,
    /// 自动闪光模式
    AutoFlash,
    /// 红眼消除模式
    RedEyeReduction,
}

/// 为LEDMode实现defmt::Format trait，使其可以在info!宏中使用
impl defmt::Format for LEDMode {
    fn format(&self, f: defmt::Formatter) {
        match self {
            LEDMode::FlashLED => defmt::write!(f, "FlashLED"),
            LEDMode::TorchLED => defmt::write!(f, "TorchLED"),
            LEDMode::AutoFlash => defmt::write!(f, "AutoFlash"),
            LEDMode::RedEyeReduction => defmt::write!(f, "RedEyeReduction"),
        }
    }
}

/// 闪光功率计算器
#[derive(Debug, Clone, Copy)]
pub struct FlashPowerCalculator {
    pub guide_number: f32, // 闪光指数
}

impl FlashPowerCalculator {
    pub fn new() -> Self {
        Self {
            guide_number: 12.0, // 典型LED闪光指数
        }
    }

    pub fn calculate(&self, distance: f32, iso: u16) -> u8 {
        // 闪光功率计算公式：功率 = (距离²) / (ISO × 闪光指数²)
        let base_power =
            (distance * distance) / (iso as f32 * self.guide_number * self.guide_number);
        let power = (base_power * 100.0).round() as u8;
        power.clamp(10, 100) // 限制在10-100%
    }
}

/// 自动闪光算法
#[derive(Debug, Clone, Copy)]
pub struct AutoFlashAlgorithm {
    pub ambient_light_threshold: u16, // 环境光阈值
    pub subject_distance: f32,        // 主体距离
    pub iso_sensitivity: u16,         // ISO感光度
    pub flash_power_calc: FlashPowerCalculator,
}

impl AutoFlashAlgorithm {
    pub fn new() -> Self {
        Self {
            ambient_light_threshold: 100, // 默认阈值
            subject_distance: 2.0,        // 默认2米
            iso_sensitivity: 100,
            flash_power_calc: FlashPowerCalculator::new(),
        }
    }

    pub fn calculate_flash_power(&self, ambient_light: u16, distance: f32) -> u8 {
        // 根据环境光和距离计算闪光功率
        let mut power = 0u8;

        if ambient_light < self.ambient_light_threshold {
            // 环境光不足，需要闪光
            power = self
                .flash_power_calc
                .calculate(distance, self.iso_sensitivity);
        }

        // 限制最大功率避免过曝
        power.min(100)
    }

    pub fn should_use_flash(&self, ambient_light: u16) -> bool {
        ambient_light < self.ambient_light_threshold
    }
}

/// LED电源管理
#[derive(Debug, Clone, Copy)]
pub struct LEDPowerManagement {
    pub total_flash_count: u32,
    pub last_flash_time: Option<Instant>,
    pub thermal_protection_level: u8,
    pub overcurrent_protection: bool,
}

impl LEDPowerManagement {
    pub fn new() -> Self {
        Self {
            total_flash_count: 0,
            last_flash_time: None,
            thermal_protection_level: 0,
            overcurrent_protection: true,
        }
    }

    /// 检查是否可以执行闪光
    pub fn can_flash(&self) -> bool {
        let current_time = Instant::now();

        // 检查冷却时间（最小间隔500ms）
        if let Some(last_flash_time) = self.last_flash_time {
            if current_time.duration_since(last_flash_time) < Duration::from_millis(500) {
                return false;
            }
        }

        // 检查热保护级别
        if self.thermal_protection_level > 80 {
            return false; // 过热保护
        }

        true
    }

    /// 热保护冷却函数
    pub fn cool_down(&mut self) {
        // 每秒钟降低10%的热保护级别
        if self.thermal_protection_level > 0 {
            self.thermal_protection_level = self.thermal_protection_level.saturating_sub(10);
        }
    }
}

impl Default for LEDPowerManagement {
    fn default() -> Self {
        Self::new()
    }
}

/// LED控制结构体
#[derive(Debug, Clone, Copy)]
pub struct LEDControl {
    pub flash_enabled: bool,
    pub torch_enabled: bool,
    pub flash_intensity: u8, // 0-100%
    pub torch_intensity: u8, // 0-100%
    pub strobe_mode: StrobeMode,
}

impl Default for LEDControl {
    fn default() -> Self {
        Self {
            flash_enabled: false,
            torch_enabled: false,
            flash_intensity: 100,
            torch_intensity: 50,
            strobe_mode: StrobeMode::Disable,
        }
    }
}

/// LED闪光时序参数
#[derive(Clone, Copy)]
pub struct FlashTiming {
    pub preflash_duration: u16,  // 预闪持续时间：1-2ms
    pub mainflash_delay: u16,    // 主闪延迟：10-15ms
    pub mainflash_duration: u16, // 主闪持续时间：50-100ms
    pub recharge_time: u16,      // 充电时间：200-500ms
}

impl Default for FlashTiming {
    fn default() -> Self {
        Self {
            preflash_duration: 2,
            mainflash_delay: 12,
            mainflash_duration: 80,
            recharge_time: 300,
        }
    }
}

/// 为FlashTiming实现defmt::Format trait，使其可以在info!宏中使用
impl defmt::Format for FlashTiming {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(
            f,
            "FlashTiming {{ preflash_duration: {}ms, mainflash_delay: {}ms, mainflash_duration: {}ms, recharge_time: {}ms }}",
            self.preflash_duration,
            self.mainflash_delay,
            self.mainflash_duration,
            self.recharge_time
        );
    }
}

/// LED规格说明
#[derive(Debug, Clone, Copy)]
pub struct LEDSpecification {
    /// 最大电流：200mA
    pub max_current: u16,
    /// 触发电压：3.3V
    pub trigger_voltage: f32,
    /// 响应时间：<100μs
    pub response_time: u16,
    /// 色温：5600K（日光白）
    pub color_temperature: u16,
}

/// 为LEDSpecification实现defmt::Format trait，使其可以在info!宏中使用
impl defmt::Format for LEDSpecification {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(
            f,
            "LEDSpecification {{ max_current: {}mA, trigger_voltage: {}V, response_time: {}μs, color_temperature: {}K }}",
            self.max_current,
            self.trigger_voltage,
            self.response_time,
            self.color_temperature
        );
    }
}

impl Default for LEDSpecification {
    fn default() -> Self {
        Self {
            max_current: 200,
            trigger_voltage: 3.3,
            response_time: 100,
            color_temperature: 5600,
        }
    }
}

/// LED Flash Controller
pub struct LEDFlashController {
    led_ctrl: LEDControl,
    spec: LEDSpecification,
    timing: FlashTiming,
    auto_flash_algo: AutoFlashAlgorithm,
    power_mgmt: LEDPowerManagement,
}

impl LEDFlashController {
    /// Create a new LED flash controller with default settings
    pub fn new() -> Self {
        Self {
            led_ctrl: LEDControl::default(),
            spec: LEDSpecification::default(),
            timing: FlashTiming::default(),
            auto_flash_algo: AutoFlashAlgorithm::new(),
            power_mgmt: LEDPowerManagement::new(),
        }
    }

    /// Create a new LED flash controller with specific mode
    pub fn with_mode(mode: LEDMode) -> Self {
        let mut ctrl = Self::new();
        ctrl.set_mode(mode);
        ctrl
    }

    /// Set LED mode
    pub fn set_mode(&mut self, mode: LEDMode) {
        info!("Setting LED mode to {:?}", mode);
        match mode {
            LEDMode::FlashLED => {
                self.led_ctrl.strobe_mode = StrobeMode::LedMode;
                self.led_ctrl.flash_enabled = true;
                self.led_ctrl.torch_enabled = false;
            }
            LEDMode::TorchLED => {
                self.led_ctrl.strobe_mode = StrobeMode::Disable;
                self.led_ctrl.flash_enabled = false;
                self.led_ctrl.torch_enabled = true;
            }
            LEDMode::AutoFlash => {
                self.led_ctrl.strobe_mode = StrobeMode::Auto;
                self.led_ctrl.flash_enabled = true;
                self.led_ctrl.torch_enabled = false;
            }
            LEDMode::RedEyeReduction => {
                self.led_ctrl.strobe_mode = StrobeMode::LedMode;
                self.led_ctrl.flash_enabled = true;
                self.led_ctrl.torch_enabled = false;
            }
        }
    }

    /// Get current LED mode
    pub fn mode(&self) -> LEDMode {
        match (
            self.led_ctrl.flash_enabled,
            self.led_ctrl.torch_enabled,
            self.led_ctrl.strobe_mode,
        ) {
            (true, false, StrobeMode::LedMode) => LEDMode::FlashLED,
            (false, true, _) => LEDMode::TorchLED,
            (true, false, StrobeMode::Auto) => LEDMode::AutoFlash,
            _ => LEDMode::FlashLED,
        }
    }

    /// Set LED specification
    pub fn set_specification(&mut self, spec: LEDSpecification) {
        info!("Setting LED specification: {:?}", spec);
        self.spec = spec;
    }

    /// Get LED specification
    pub fn specification(&self) -> LEDSpecification {
        self.spec
    }

    /// Set flash timing parameters
    pub fn set_timing(&mut self, timing: FlashTiming) {
        info!("Setting flash timing: {:?}", timing);
        self.timing = timing;
    }

    /// Get flash timing parameters
    pub fn timing(&self) -> FlashTiming {
        self.timing
    }

    /// Set flash intensity (0-100%)
    pub fn set_flash_intensity(&mut self, intensity: u8) {
        self.led_ctrl.flash_intensity = intensity.min(100);
        info!(
            "Setting flash intensity to {}%",
            self.led_ctrl.flash_intensity
        );
    }

    /// Set torch intensity (0-100%)
    pub fn set_torch_intensity(&mut self, intensity: u8) {
        self.led_ctrl.torch_intensity = intensity.min(100);
        info!(
            "Setting torch intensity to {}%",
            self.led_ctrl.torch_intensity
        );
    }

    /// Set auto flash algorithm parameters
    pub fn set_auto_flash_algorithm(&mut self, algo: AutoFlashAlgorithm) {
        self.auto_flash_algo = algo;
    }

    /// Get auto flash algorithm
    pub fn auto_flash_algorithm(&self) -> &AutoFlashAlgorithm {
        &self.auto_flash_algo
    }

    /// Initialize LED flash functionality
    pub fn init(&mut self) -> Result<(), &'static str> {
        info!("Initializing LED flash functionality");

        // 配置STROBE引脚相关的寄存器
        // 根据用户提供的详细配置信息:
        // TODO: 实际的寄存器配置需要根据OV5640数据手册进行
        // 示例代码如下：
        // self.write_i2c(0x300A, 0x01)?;     // 启用STROBE功能
        // self.write_i2c(0x3A20, 0x84)?;     // STROBE选项配置
        // self.write_i2c(0x3A21, 0x78)?;     // 插入帧数控制
        // self.write_i2c(0x3A08, 0x01)?;     // LED模式曝光值添加
        // self.write_i2c(0x3A1D, 0x18)?;     // LED模式行数添加（低字节）
        // self.write_i2c(0x3A1E, 0x68)?;     // 稳定范围下限
        // self.write_i2c(0x3A1F, 0x40)?;     // 快速区域下限
        // self.write_i2c(0x3A00, 0x58)?;     // AEC系统控制（包含LED设置）

        info!("LED flash functionality initialized");
        Ok(())
    }

    /// 设置LED闪光时的曝光补偿
    pub fn set_led_exposure_compensation(&mut self) -> Result<(), &'static str> {
        info!("Setting LED exposure compensation");

        // TODO: 实际的寄存器配置需要根据OV5640数据手册进行
        // 示例代码如下：
        // self.write_i2c(0x3A08, 0x01)?;     // 开启LED模式曝光补偿
        // self.write_i2c(0x3A1D, 0x18)?;     // LED添加行数[7:0]
        // self.write_i2c(0x3A02, 0x03)?;     // 60Hz最大曝光
        // self.write_i2c(0x3A03, 0xD8)?;     // 60Hz最大曝光
        // self.write_i2c(0x3A14, 0x02)?;     // 50Hz最大曝光
        // self.write_i2c(0x3A15, 0x50)?;     // 50Hz最大曝光

        info!("LED exposure compensation set");
        Ok(())
    }

    /// 设置闪光灯模式
    pub fn set_flash_mode(&mut self, mode: StrobeMode, intensity: u8) -> Result<(), &'static str> {
        self.led_ctrl.strobe_mode = mode;
        self.led_ctrl.flash_intensity = intensity.min(100);

        info!(
            "Setting flash mode to {:?} with intensity {}%",
            mode, intensity
        );

        // 配置STROBE寄存器
        let strobe_reg = match mode {
            StrobeMode::LedMode => 0x01,
            StrobeMode::XenonMode => 0x02,
            StrobeMode::Auto => 0x04,
            StrobeMode::Disable => 0x00,
        };

        // TODO: 写入STROBE控制寄存器
        // self.write_i2c(0x300A, strobe_reg)?;

        // 设置LED强度（在实际硬件中会转换为PWM占空比）
        info!("Flash intensity set to {}%", self.led_ctrl.flash_intensity);

        Ok(())
    }

    /// 手电筒模式控制
    pub fn set_torch_mode(&mut self, enable: bool, intensity: u8) -> Result<(), &'static str> {
        self.led_ctrl.torch_enabled = enable;
        self.led_ctrl.torch_intensity = intensity.min(100);

        info!(
            "Setting torch mode to {} with intensity {}%",
            enable, intensity
        );

        if enable {
            // 在实际实现中，这里会设置较低的亮度以避免过热
            info!(
                "Torch mode enabled with {}% intensity",
                self.led_ctrl.torch_intensity
            );
        } else {
            info!("Torch mode disabled");
        }

        Ok(())
    }

    /// 根据环境光和距离自动设置闪光参数
    pub fn auto_configure_flash(&mut self, ambient_light: u16, distance: f32) {
        if self.auto_flash_algo.should_use_flash(ambient_light) {
            let power = self
                .auto_flash_algo
                .calculate_flash_power(ambient_light, distance);
            self.set_flash_intensity(power);
            info!("Auto flash configured with {}% intensity", power);
        } else {
            info!("Sufficient ambient light, flash not needed");
        }
    }

    /// 带保护的闪光执行
    pub async fn flash_with_protection(
        &mut self,
        intensity: u8,
        duration_ms: u16,
    ) -> Result<(), &'static str> {
        if !self.power_mgmt.can_flash() {
            return Err("Cannot flash due to protection mechanisms");
        }

        // 强度和时间限制
        let intensity = intensity.min(100);
        let duration_ms = duration_ms.min(1000);

        // 执行闪光
        self.set_flash_mode(StrobeMode::LedMode, intensity)?;
        self.control_strobe_pin(true).await?;
        Timer::after(Duration::from_millis(duration_ms as u64)).await;
        self.set_flash_mode(StrobeMode::Disable, 0)?;
        self.control_strobe_pin(false).await?;

        // 更新计数和时间
        self.power_mgmt.total_flash_count += 1;
        self.power_mgmt.last_flash_time = Some(Instant::now());

        // 更新热保护级别（每次闪光增加5%）
        self.power_mgmt.thermal_protection_level =
            (self.power_mgmt.thermal_protection_level + 5).min(100);

        Ok(())
    }

    /// 热保护冷却函数
    pub fn cool_down(&mut self) {
        self.power_mgmt.cool_down();
    }

    /// 获取电源管理状态
    pub fn power_management(&self) -> &LEDPowerManagement {
        &self.power_mgmt
    }

    /// 触发LED闪光（完整流程）
    /// 包括配置OV5640内部寄存器和控制XL9555的GPIO
    pub async fn trigger(&mut self) -> Result<(), &'static str> {
        info!("Triggering complete LED flash sequence");

        // 1. 首先确保OV5640内部寄存器已正确配置
        self.init()?;

        match self.mode() {
            LEDMode::RedEyeReduction => {
                // 红眼消除模式需要预闪+主闪的双脉冲序列
                // 预闪
                self.control_strobe_pin(true).await?;
                Timer::after(Duration::from_millis(self.timing.preflash_duration as u64)).await;
                self.control_strobe_pin(false).await?;

                // 等待主闪延迟
                Timer::after(Duration::from_millis(self.timing.mainflash_delay as u64)).await;

                // 主闪
                self.control_strobe_pin(true).await?;
                Timer::after(Duration::from_millis(self.timing.mainflash_duration as u64)).await;
                self.control_strobe_pin(false).await?;
            }
            LEDMode::FlashLED => {
                // 普通闪光模式
                self.control_strobe_pin(true).await?;
                Timer::after(Duration::from_millis(self.timing.mainflash_duration as u64)).await;
                self.control_strobe_pin(false).await?;
            }
            LEDMode::TorchLED => {
                // 手电筒模式，持续照明
                self.control_strobe_pin(true).await?;
            }
            LEDMode::AutoFlash => {
                // 自动闪光模式，根据环境光自动调整
                self.control_strobe_pin(true).await?;
                Timer::after(Duration::from_millis(self.timing.mainflash_duration as u64)).await;
                self.control_strobe_pin(false).await?;
            }
        }

        Ok(())
    }

    /// 控制STROBE引脚
    /// 这个函数直接控制连接到OV5640 STROBE引脚的XL9555 GPIO
    async fn control_strobe_pin(&mut self, state: bool) -> Result<(), &'static str> {
        info!("Controlling STROBE pin: {}", state);

        // STROBE引脚连接到XL9555的P0.6 (GBC_LED_IO)
        // 这需要通过I2C接口控制XL9555的输出寄存器
        crate::xl9555::control_strobe_pin(state)
            .await
            .map_err(|_| "Failed to control STROBE pin")
    }

    /// 关闭LED闪光灯
    pub fn disable(&mut self) -> Result<(), &'static str> {
        info!("Disabling LED flash");

        // 禁用闪光灯功能
        // TODO: 根据OV5640数据手册禁用相关寄存器
        // 示例代码如下：
        // self.write_i2c(0x300A, StrobeMode::Disable as u8)?; // 禁用STROBE

        Ok(())
    }
}

impl Default for LEDFlashController {
    fn default() -> Self {
        Self::new()
    }
}
