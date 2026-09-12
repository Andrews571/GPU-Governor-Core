//! 宽松整数反序列化
//!
//! 配置文件中的整数型字段允许被写成"整数值的浮点"（如 `0.0`、`999.0`）或数字字符串。
//!
//! 历史背景：WebUI 使用的 TOML 序列化库默认不把 JS 的 `number` 当作 TOML 整数，
//! 会把 `0` 写成 `0.0`。已经落盘的配置文件只能用宽容解析读回，
//! 否则整份配置会解析失败，进而导致模式切换和参数下发全部失效。

use std::{fmt::Formatter, marker::PhantomData};

use serde::de::{self, Deserializer, Visitor};

/// 支持宽松解析的整数类型。
trait LenientInt: Sized {
    /// 类型名称，用于错误信息。
    const NAME: &'static str;

    fn from_i64(value: i64) -> Option<Self>;

    fn from_u64(value: u64) -> Option<Self>;

    /// 由"整数值浮点"转换，调用方需保证已通过有限性和整数性检查。
    fn from_valid_f64(value: f64) -> Option<Self>;
}

impl LenientInt for i32 {
    const NAME: &'static str = "i32";

    fn from_i64(value: i64) -> Option<Self> {
        Self::try_from(value).ok()
    }

    fn from_u64(value: u64) -> Option<Self> {
        Self::try_from(value).ok()
    }

    fn from_valid_f64(value: f64) -> Option<Self> {
        (value >= Self::MIN as f64 && value <= Self::MAX as f64).then_some(value as Self)
    }
}

impl LenientInt for i64 {
    const NAME: &'static str = "i64";

    fn from_i64(value: i64) -> Option<Self> {
        Some(value)
    }

    fn from_u64(value: u64) -> Option<Self> {
        Self::try_from(value).ok()
    }

    fn from_valid_f64(value: f64) -> Option<Self> {
        (value >= Self::MIN as f64 && value <= Self::MAX as f64).then_some(value as Self)
    }
}

impl LenientInt for u64 {
    const NAME: &'static str = "u64";

    fn from_i64(value: i64) -> Option<Self> {
        Self::try_from(value).ok()
    }

    fn from_u64(value: u64) -> Option<Self> {
        Some(value)
    }

    fn from_valid_f64(value: f64) -> Option<Self> {
        (value >= 0.0 && value <= Self::MAX as f64).then_some(value as Self)
    }
}

/// 通用的宽松整数访问器。
struct LenientIntVisitor<T>(PhantomData<T>);

impl<'de, T> Visitor<'de> for LenientIntVisitor<T>
where
    T: LenientInt,
{
    type Value = T;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "an integer, an integer-like float (e.g. 999.0), or a numeric string, for {}",
            T::NAME
        )
    }

    fn visit_i64<E>(self, value: i64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        T::from_i64(value).ok_or_else(|| E::custom(format!("integer out of range for {}", T::NAME)))
    }

    fn visit_u64<E>(self, value: u64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        T::from_u64(value).ok_or_else(|| E::custom(format!("integer out of range for {}", T::NAME)))
    }

    fn visit_f64<E>(self, value: f64) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        if !value.is_finite() {
            return Err(E::custom("floating point value is not finite"));
        }
        if value.fract() != 0.0 {
            return Err(E::custom("floating point value is not an integer"));
        }
        T::from_valid_f64(value)
            .ok_or_else(|| E::custom(format!("integer out of range for {}", T::NAME)))
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        let out_of_range = || E::custom(format!("integer out of range for {}", T::NAME));
        let trimmed = value.trim();
        if let Ok(parsed) = trimmed.parse::<i64>() {
            return T::from_i64(parsed).ok_or_else(out_of_range);
        }
        if let Ok(parsed) = trimmed.parse::<u64>() {
            return T::from_u64(parsed).ok_or_else(out_of_range);
        }
        let parsed = trimmed
            .parse::<f64>()
            .map_err(|_| E::custom("string is not a valid number"))?;
        self.visit_f64(parsed)
    }

    fn visit_string<E>(self, value: String) -> std::result::Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_str(&value)
    }
}

macro_rules! lenient_int_deserializer {
    ($function:ident, $ty:ty) => {
        /// 宽松地反序列化整数：接受整数、整数值浮点（如 `999.0`）或数字字符串。
        pub fn $function<'de, D>(deserializer: D) -> std::result::Result<$ty, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(LenientIntVisitor::<$ty>(PhantomData))
        }
    };
}

lenient_int_deserializer!(de_i32_lenient, i32);
lenient_int_deserializer!(de_i64_lenient, i64);
lenient_int_deserializer!(de_u64_lenient, u64);
