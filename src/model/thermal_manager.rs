//! GPU热管理模块 - 主动式热降频，基于thermal zone温度反馈
//!
//! 该模块在GPU governor造成节流之前主动收紧margin，
//! 减少MediaTek厂商thermal framework突然限频导致的掉帧。
//!
//! ⚠️ 阈值(WARN_TEMP_C/CRIT_TEMP_C)和thermal zone匹配关键字均需要
//! 在实际设备上验证 - 不同内核/设备的thermal zone命名不统一。

use std::fs;

use anyhow::{Result, anyhow};
use log::debug;
use once_cell::sync::OnceCell;

/// 开始降低margin的温度（摄氏度）
const WARN_TEMP_C: f64 = 42.0;
/// margin降到最低值的温度（摄氏度）
const CRIT_TEMP_C: f64 = 48.0;
/// 高温时margin允许降到的最小值
const MIN_MARGIN_FLOOR: i64 = -20;

/// 缓存已发现的thermal zone路径，避免每次采样都扫描/sys
static GPU_ZONE_PATH: OnceCell<Option<String>> = OnceCell::new();

/// 扫描 /sys/class/thermal/thermal_zone*/type 寻找GPU相关的zone
///
/// 关键字为猜测性质，来自常见MediaTek命名（tsgpu、gpu、soc_gpu等），
/// 未在Dimensity 7300 Ultra上逐一验证 - 部署前请用
/// `for z in /sys/class/thermal/thermal_zone*; do cat $z/type; done`
/// 确认实际存在的zone名称，必要时调整下面的关键字列表。
fn discover_gpu_zone() -> Option<String> {
    let entries = fs::read_dir("/sys/class/thermal").ok()?;
    let candidates = ["gpu", "tsgpu", "soc_gpu", "gpuss"];

    for entry in entries.flatten() {
        let path = entry.path();
        let type_path = path.join("type");
        if let Ok(zone_type) = fs::read_to_string(&type_path) {
            let zone_type_lower = zone_type.to_lowercase();
            if candidates.iter().any(|c| zone_type_lower.contains(c)) {
                let temp_path = path.join("temp");
                if temp_path.exists() {
                    debug!(
                        "GPU thermal zone found: {} (type: {})",
                        temp_path.display(),
                        zone_type.trim()
                    );
                    return Some(temp_path.to_string_lossy().to_string());
                }
            }
        }
    }

    None
}

/// 读取GPU温度，单位摄氏度
///
/// thermal zone的temp节点标准单位是毫摄氏度(millidegree)，
/// 部分MediaTek设备可能直接是摄氏度 - 如果读到的值异常
/// (例如恒定在40-50而不是随负载变化)，需要去掉/1000.0的换算。
pub fn get_gpu_temp_c() -> Result<f64> {
    let path = GPU_ZONE_PATH
        .get_or_init(discover_gpu_zone)
        .as_ref()
        .ok_or_else(|| anyhow!("No GPU thermal zone found"))?;

    let raw = fs::read_to_string(path)?;
    let millidegree: f64 = raw.trim().parse()?;

    Ok(millidegree / 1000.0)
}

/// 根据当前GPU温度调整margin，实现主动降频
///
/// 温度低于WARN_TEMP_C：不干预，返回原始margin
/// 温度介于WARN_TEMP_C和CRIT_TEMP_C之间：线性插值降低margin
/// 温度达到或超过CRIT_TEMP_C：margin钳制在MIN_MARGIN_FLOOR
///
/// 读取thermal zone失败时静默返回原始margin，不影响正常调频。
pub fn apply_thermal_derate(base_margin: i64) -> i64 {
    let temp_c = match get_gpu_temp_c() {
        Ok(t) => t,
        Err(e) => {
            debug!("Thermal read failed, skipping derate: {e}");
            return base_margin;
        }
    };

    if temp_c < WARN_TEMP_C {
        return base_margin;
    }

    let ratio = ((temp_c - WARN_TEMP_C) / (CRIT_TEMP_C - WARN_TEMP_C)).clamp(0.0, 1.0);
    let derated = base_margin as f64 - ratio * (base_margin - MIN_MARGIN_FLOOR) as f64;
    let result = derated as i64;

    debug!("Thermal derate: {temp_c:.1}°C, margin {base_margin} -> {result}");
    result
}
