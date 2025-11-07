use core::cell::RefCell;
use critical_section::Mutex;
use defmt::info;
use embassy_time::Timer;
use esp_hal::gpio::interconnect::PeripheralOutput;
use esp_hal::i2c::master::Config as I2cConfig;
use esp_hal::i2c::master::{I2c, Instance};
use esp_hal::Blocking;

pub const XL9555_ADDR: u8 = 0x20; // 7-bit I2C 地址

static I2C: Mutex<RefCell<Option<I2c<Blocking>>>> = Mutex::new(RefCell::new(None));
// 在全局静态变量中添加按键状态跟踪
// [KEY0, KEY1, KEY2, KEY3]
static KEY_STATES: Mutex<RefCell<[bool; 4]>> = Mutex::new(RefCell::new([false; 4]));
// 添加背光状态跟踪
static BL_STATE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(true));

// 寄存器地址
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

// IO 位定义 (P0: bit 0~7, P1: bit 8~15)
pub mod io_bits {
    // pub const AP_INT_IO: u16 = 0x0001; // P0.0
    // pub const QMA_INT_IO: u16 = 0x0002; // P0.1
    // pub const SPK_EN_IO: u16 = 0x0004; // P0.2
    // pub const BEEP_IO: u16 = 0x0008; // P0.3
    // pub const OV_PWDN_IO: u16 = 0x0010; // P0.4
    // pub const OV_RESET_IO: u16 = 0x0020; // P0.5
    // pub const GBC_LED_IO: u16 = 0x0040; // P0.6
    // pub const GBC_KEY_IO: u16 = 0x0080; // P0.7
    pub const LCD_BL_IO: u16 = 0x0100; // P1.0
    // pub const CT_RST_IO: u16 = 0x0200; // P1.1
    pub const SLCD_RST_IO: u16 = 0x0400; // P1.2
    pub const SLCD_PWR_IO: u16 = 0x0800; // P1.3
    pub const KEY3_IO: u16 = 0x1000; // P1.4
    pub const KEY2_IO: u16 = 0x2000; // P1.5
    pub const KEY1_IO: u16 = 0x4000; // P1.6
    pub const KEY0_IO: u16 = 0x8000; // P1.7
}

pub async fn init(
    i2c: impl Instance + 'static,
    sda: impl PeripheralOutput<'static>,
    scl: impl PeripheralOutput<'static>,
) {
    let mut i2c = I2c::new(i2c, I2cConfig::default())
        .expect("Failed to initialize I2C")
        .with_sda(sda)
        .with_scl(scl);

    // 配置XL9555 IO方向 (0表示输出，1表示输入)
    // P0全部配置为输入 (按键等)
    // P1配置为输出，但按键引脚配置为输入
    // P1.0-P1.3 为输出（LCD控制）
    // P1.4-P1.7 为输入（按键）
    i2c.write(XL9555_ADDR, &[registers::CONFIG_PORT_0, 0xFF])
        .expect("Failed to configure XL9555 PORT0");
    i2c.write(XL9555_ADDR, &[registers::CONFIG_PORT_1, 0xF0])
        .expect("Failed to configure XL9555 PORT1");

    // 初始化输出端口状态
    i2c.write(XL9555_ADDR, &[registers::OUTPUT_PORT_0, 0x00])
        .ok();
    i2c.write(XL9555_ADDR, &[registers::OUTPUT_PORT_1, 0x00])
        .ok();

    critical_section::with(|cs| {
        I2C.borrow_ref_mut(cs).replace(i2c);
    });
}

// 控制 SPI LCD 电源状态的函数
pub fn set_spi_lcd_power_state(i2c: &mut I2c<Blocking>, state: bool) {
    // 读取当前端口1输出状态
    let mut port1_data = [0u8];
    if i2c
        .write_read(XL9555_ADDR, &[registers::OUTPUT_PORT_1], &mut port1_data)
        .is_ok()
    {
        // 根据状态设置 SPI LCD 电源引脚 (P1.3)
        let new_port1_data = if state {
            port1_data[0] | (io_bits::SLCD_PWR_IO >> 8) as u8 // 设置P1.3为高电平
        } else {
            port1_data[0] & !((io_bits::SLCD_PWR_IO >> 8) as u8) // 设置P1.3为低电平
        };

        // 写回端口1输出
        i2c.write(XL9555_ADDR, &[registers::OUTPUT_PORT_1, new_port1_data])
            .ok();
    }
}

// 控制 SPI LCD 复位状态的函数
pub fn set_spi_lcd_reset_state(i2c: &mut I2c<Blocking>, state: bool) {
    // 读取当前端口1输出状态
    let mut port1_data = [0u8];
    if i2c
        .write_read(XL9555_ADDR, &[registers::OUTPUT_PORT_1], &mut port1_data)
        .is_ok()
    {
        // 根据状态设置 SPI LCD 复位引脚 (P1.2)
        let new_port1_data = if state {
            port1_data[0] | (io_bits::SLCD_RST_IO >> 8) as u8 // 设置P1.2为高电平
        } else {
            port1_data[0] & !((io_bits::SLCD_RST_IO >> 8) as u8) // 设置P1.2为低电平
        };

        // 写回端口1输出
        i2c.write(XL9555_ADDR, &[registers::OUTPUT_PORT_1, new_port1_data])
            .ok();
    }
}

// 添加公共函数用于外部调用
pub fn spi_lcd_reset(state: bool) {
    critical_section::with(|cs| {
        let mut i2c = I2C.borrow_ref_mut(cs);
        let i2c_ref = i2c.as_mut().unwrap();
        set_spi_lcd_reset_state(i2c_ref, state);
    });
}

/// 控制 LCD 背光开关
/// 根据描述，PWR 引脚带有下拉电阻，当引脚被拉低或悬空时背光关闭，当引脚被拉高时背光点亮
pub fn set_lcd_backlight(state: bool) {
    critical_section::with(|cs| {
        let mut i2c = I2C.borrow_ref_mut(cs);
        let i2c_ref = i2c.as_mut().unwrap();
        set_spi_lcd_power_state(i2c_ref, state);
    });
}

/// 初始化ATK-MD0240模块
/// 执行硬件复位序列：RST引脚拉低至少10微秒，然后拉高并延时120毫秒等待复位完成
pub async fn init_atk_md0240() {
    // 拉低RST引脚至少10微秒
    spi_lcd_reset(false);
    Timer::after_micros(10).await;

    // 拉高RST引脚
    spi_lcd_reset(true);

    // 延时120毫秒等待复位完成
    Timer::after_millis(120).await;
}

/**
* 读取按键输入
* 状态跟踪: 添加 KEY_STATES 全局变量记录每个按键的上一次状态
* 边缘检测: 只有当按键从释放状态(高电平)变为按下状态(低电平)时才触发事件
* 状态更新: 每次循环结束后更新按键状态数组
* 这样修改后，即使按键持续按下也只会触发一次日志输出，直到按键释放后再次按下才会重新触发
* 硬件连接：
* iic_int (XL9555中断引脚) 连接到 ESP32 的 GPIO0
* GPIO0 同时也是 BOOT_BUTTON 的引脚
* 中断触发机制：
* 当 KEY0-KEY3 按下时，XL9555 通过 iic_int 引脚产生中断信号
* 该信号传递到 GPIO0，触发了已注册的中断处理程序
* 中断处理程序中会切换 LED 状态
*/
#[embassy_executor::task]
pub async fn read_keys() {
    loop {
        critical_section::with(|cs| {
            let mut i2c = I2C.borrow_ref_mut(cs);
            let i2c_ref = i2c.as_mut().unwrap();

            // 读取端口0和端口1的输入值
            let mut port0_data = [0u8];
            let mut port1_data = [0u8];

            i2c_ref
                .write_read(XL9555_ADDR, &[registers::INPUT_PORT_0], &mut port0_data)
                .ok();
            i2c_ref
                .write_read(XL9555_ADDR, &[registers::INPUT_PORT_1], &mut port1_data)
                .ok();

            let key_value: u16 = (port1_data[0] as u16) << 8 | (port0_data[0] as u16);

            // 获取当前按键状态（低电平表示按下）
            let current_states = [
                (key_value & io_bits::KEY0_IO) == 0,
                (key_value & io_bits::KEY1_IO) == 0,
                (key_value & io_bits::KEY2_IO) == 0,
                (key_value & io_bits::KEY3_IO) == 0,
            ];

            // 检查按键状态变化
            let mut key_states = KEY_STATES.borrow_ref_mut(cs);
            for i in 0..4 {
                if current_states[i] && !key_states[i] {
                    // 按键刚被按下
                    match i {
                        0 => info!("KEY0 pressed"),
                        1 => {
                            info!("KEY1 pressed - toggling LCD backlight");
                            // 切换背光状态
                            let mut bl_state = BL_STATE.borrow_ref_mut(cs);
                            *bl_state = !*bl_state;
                            set_spi_lcd_power_state(i2c_ref, *bl_state);
                            info!(
                                "LCD backlight is now {}",
                                if *bl_state { "ON" } else { "OFF" }
                            );
                        }
                        2 => info!("KEY2 pressed"),
                        3 => info!("KEY3 pressed"),
                        _ => {}
                    }
                }
            }

            // 更新按键状态
            *key_states = current_states;
        });

        Timer::after_millis(50).await;
    }
}
