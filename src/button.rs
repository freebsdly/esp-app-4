use crate::i2c;
use crate::xl9555::{io_bits, read_input_ports, set_spi_lcd_power_state};
use defmt::{error, info};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex as EmbassyMutex;
use embassy_time::Timer;
use esp_hal::gpio::{Event, Input, InputConfig, InputPin};

pub static BOOT_BUTTON_ASYNC: EmbassyMutex<CriticalSectionRawMutex, Option<Input<'static>>> =
    EmbassyMutex::new(None);

// 在全局静态变量中添加按键状态跟踪
// [KEY0, KEY1, KEY2, KEY3]
static KEY_STATES: EmbassyMutex<CriticalSectionRawMutex, [bool; 4]> = EmbassyMutex::new([false; 4]);
// 添加背光状态跟踪
static BL_STATE: EmbassyMutex<CriticalSectionRawMutex, bool> = EmbassyMutex::new(true);

pub async fn boot_button_init(button: impl InputPin + 'static) {
    let mut boot_button = Input::new(button, InputConfig::default());
    boot_button.listen(Event::FallingEdge);
    BOOT_BUTTON_ASYNC.lock().await.replace(boot_button);
    info!("Boot button initialized")
}

/// 按键输入检测任务
///
/// 该异步任务负责持续检测 XL9555 连接的按键状态
/// 使用轮询方式每 50 毫秒检测一次按键状态
/// 实现边缘检测，确保按键按下时只触发一次操作
///
/// 按键功能分配：
/// - KEY0: 未分配特定功能
/// - KEY1: 切换 LCD 背光状态
/// - KEY2: 未分配特定功能
/// - KEY3: 未分配特定功能
///
/// 读取按键输入
/// 状态跟踪: 添加 KEY_STATES 全局变量记录每个按键的上一次状态
/// 边缘检测: 只有当按键从释放状态(高电平)变为按下状态(低电平)时才触发事件
/// 状态更新: 每次循环结束后更新按键状态数组
/// 这样修改后，即使按键持续按下也只会触发一次日志输出，直到按键释放后再次按下才会重新触发
/// 硬件连接：
/// iic_int (XL9555中断引脚) 连接到 ESP32 的 GPIO0
/// GPIO0 同时也是 BOOT_BUTTON 的引脚
/// 中断触发机制：
/// 当 KEY0-KEY3 按下时，XL9555 通过 iic_int 引脚产生中断信号
/// 该信号传递到 GPIO0，触发了已注册的中断处理程序
/// 中断处理程序中会切换 LED 状态
///
#[embassy_executor::task]
pub async fn read_keys() {
    loop {
        i2c::with_i2c(|i2c_ref| {
            // 读取 P0 和 P1 端口输入状态
            let (port0_data, port1_data) = read_input_ports(i2c_ref);

            // 组合按键值
            //将 P1 和 P0 端口的值组合成一个 16 位值用于按键检测
            // 高 8 位来自 P1 端口，低 8 位来自 P0 端口
            let key_value: u16 = (port1_data[0] as u16) << 8 | (port0_data[0] as u16);

            // 获取当前按键状态（低电平表示按下）
            let current_states = [
                (key_value & io_bits::KEY0_IO) == 0,
                (key_value & io_bits::KEY1_IO) == 0,
                (key_value & io_bits::KEY2_IO) == 0,
                (key_value & io_bits::KEY3_IO) == 0,
            ];

            // 检查按键状态变化
            // 使用新的 EmbassyMutex API 替代 critical_section
            let mut key_states = KEY_STATES.try_lock().unwrap();
            for i in 0..4 {
                if current_states[i] && !key_states[i] {
                    // 按键刚被按下
                    match i {
                        0 => info!("KEY0 pressed"),
                        1 => {
                            info!("KEY1 pressed - toggling LCD backlight");
                            // 切换背光状态
                            drop(key_states); // 释放锁，以便获取背光状态锁
                            let mut bl_state = BL_STATE.try_lock().unwrap();
                            let new_bl_state = !*bl_state;
                            *bl_state = new_bl_state;
                            drop(bl_state); // 释放锁，以便在下面的调用中不会死锁

                            let result = set_spi_lcd_power_state(i2c_ref, new_bl_state);
                            if result.is_err() {
                                error!(
                                    "Failed to set LCD backlight state: {}",
                                    result.unwrap_err()
                                );
                            }
                            info!(
                                "LCD backlight is now {}",
                                if new_bl_state { "ON" } else { "OFF" }
                            );
                            key_states = KEY_STATES.try_lock().unwrap(); // 重新获取锁
                        }
                        2 => info!("KEY2 pressed"),
                        3 => info!("KEY3 pressed"),
                        _ => {}
                    }
                }
            }

            // 更新按键状态
            *key_states = current_states;

            Ok(())
        })
        .await
        .ok();

        Timer::after_millis(50).await;
    }
}
