//! 背光控制模块
//! 
//! 该模块定义了背光控制的统一接口和实现，支持直接PWM控制和通过IO扩展器控制两种方式。

use esp_hal::i2c::master::Error as I2cError;

/// 背光控制状态枚举
#[derive(Debug, Clone, Copy)]
pub enum BacklightState {
    Off,
    On(u8), // 亮度百分比
}

/// 背光控制trait接口
/// 
/// 定义了背光控制的统一接口，所有背光控制器都应实现此trait
pub trait BacklightControl {
    /// 设置背光亮度
    /// 
    /// # 参数
    /// * `brightness` - 亮度百分比，0表示关闭，100表示最亮
    /// 
    /// # 返回值
    /// * `Ok(())` - 设置成功
    /// * `Err` - 设置失败
    async fn set_brightness(&mut self, brightness: u8) -> Result<(), I2cError>;
    
    /// 关闭背光
    /// 
    /// # 返回值
    /// * `Ok(())` - 关闭成功
    /// * `Err` - 关闭失败
    async fn turn_off(&mut self) -> Result<(), I2cError> {
        self.set_brightness(0).await
    }
    
    /// 打开背光（默认亮度）
    /// 
    /// # 返回值
    /// * `Ok(())` - 打开成功
    /// * `Err` - 打开失败
    async fn turn_on(&mut self) -> Result<(), I2cError> {
        self.set_brightness(100).await
    }
    
    /// 获取当前背光状态
    /// 
    /// # 返回值
    /// * `BacklightState` - 当前背光状态
    fn get_state(&self) -> BacklightState;
}

/// XL9555背光控制器
/// 
/// 通过I2C接口控制XL9555 GPIO扩展器来控制背光
/// 背光控制连接在XL9555的P1.0引脚(GPIO10)
pub struct Xl9555Backlight {
    state: BacklightState,
}

impl Xl9555Backlight {
    /// XL9555 I2C地址
    const XL9555_ADDR: u8 = 0x20;
    /// LCD背光控制引脚 (P1.0)
    const LCD_BL_IO: u16 = 0x0100;

    /// 创建新的XL9555背光控制器实例
    pub fn new() -> Self {
        Self {
            state: BacklightState::Off,
        }
    }
    
    /// 通过XL9555设置背光控制引脚的状态
    async fn set_backlight_pin(&self, state: bool) -> Result<(), I2cError> {
        crate::xl9555::set_lcd_backlight(state).await
    }
}

impl BacklightControl for Xl9555Backlight {
    async fn set_brightness(&mut self, brightness: u8) -> Result<(), I2cError> {
        // 对于XL9555，我们只支持开/关控制，不支持PWM调光
        let state = brightness > 0;
        
        // 调用XL9555模块的函数控制背光
        self.set_backlight_pin(state).await?;
        
        // 更新状态
        self.state = if state {
            BacklightState::On(brightness)
        } else {
            BacklightState::Off
        };
        
        Ok(())
    }
    
    fn get_state(&self) -> BacklightState {
        self.state
    }
}

impl Default for Xl9555Backlight {
    fn default() -> Self {
        Self::new()
    }
}

/// 背光管理器
/// 
/// 统一管理不同类型的背光控制
pub struct BacklightManager<T: BacklightControl> {
    controller: T,
    current_state: BacklightState,
}

impl<T: BacklightControl> BacklightManager<T> {
    /// 创建新的背光管理器
    /// 
    /// # 参数
    /// * `controller` - 背光控制器实例
    /// 
    /// # 返回值
    /// * `BacklightManager` - 背光管理器实例
    pub fn new(controller: T) -> Self {
        Self {
            controller,
            current_state: BacklightState::Off,
        }
    }

    /// 统一背光控制接口
    /// 
    /// # 参数
    /// * `brightness` - 亮度百分比，0表示关闭，非0值表示开启
    /// 
    /// # 返回值
    /// * `Ok(())` - 设置成功
    /// * `Err` - 设置失败
    pub async fn set_backlight(&mut self, brightness: u8) -> Result<(), I2cError> {
        self.controller.set_brightness(brightness).await?;
        self.current_state = self.controller.get_state();
        Ok(())
    }

    /// 自动背光调节（根据环境光传感器）
    /// 
    /// # 参数
    /// * `ambient_light` - 环境光强度
    /// 
    /// # 返回值
    /// * `Ok(())` - 设置成功
    /// * `Err` - 设置失败
    pub async fn auto_adjust(&mut self, ambient_light: u16) -> Result<(), I2cError> {
        let target_brightness = self.calculate_auto_brightness(ambient_light);
        self.set_backlight(target_brightness).await
    }

    /// 根据环境光计算目标亮度
    /// 
    /// # 参数
    /// * `ambient_light` - 环境光强度
    /// 
    /// # 返回值
    /// * `u8` - 目标亮度百分比
    fn calculate_auto_brightness(&self, ambient_light: u16) -> u8 {
        // 简单的自动亮度算法
        match ambient_light {
            0..=100 => 20,    // 黑暗环境：低亮度
            101..=500 => 50,  // 一般环境：中等亮度
            501..=1000 => 80, // 明亮环境：高亮度
            _ => 100,         // 强光环境：最大亮度
        }
    }

    /// 获取当前背光状态
    /// 
    /// # 返回值
    /// * `BacklightState` - 当前背光状态
    pub fn get_state(&self) -> BacklightState {
        self.current_state
    }
    
    /// 获取内部控制器的不可变引用
    /// 
    /// # 返回值
    /// * `&T` - 控制器引用
    pub fn get_controller(&self) -> &T {
        &self.controller
    }
    
    /// 获取内部控制器的可变引用
    /// 
    /// # 返回值
    /// * `&mut T` - 控制器可变引用
    pub fn get_controller_mut(&mut self) -> &mut T {
        &mut self.controller
    }
}