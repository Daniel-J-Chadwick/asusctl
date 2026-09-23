// TODO: Generic builtin modes
// TODO: Traits for finding device + writing generic modes
// TODO: Traits for writing aura_sync
// TODO: separate keyboard and laptop parts?

use std::fmt::Debug;

use serde::{Deserialize, Serialize};
#[cfg(feature = "dbus")]
use zbus::zvariant::{OwnedValue, Type, Value};

/// A container of images/grids/gifs/pauses which can be iterated over to
/// generate cool effects
pub mod effects;

mod builtin_modes;
pub use builtin_modes::*;

/// Helper for detecting what is available
pub mod aura_detection;
pub mod error;
pub mod usb;

pub mod gz302_rear;
pub mod keyboard;

pub const AURA_LAPTOP_LED_MSG_LEN: usize = 17;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const RED: Colour = Colour {
    r: 0xff,
    g: 0x00,
    b: 0x00,
};
pub const GREEN: Colour = Colour {
    r: 0x00,
    g: 0xff,
    b: 0x00,
};
pub const BLUE: Colour = Colour {
    r: 0x00,
    g: 0x00,
    b: 0xff,
};
pub const VIOLET: Colour = Colour {
    r: 0x9b,
    g: 0x26,
    b: 0xb6,
};
pub const TEAL: Colour = Colour {
    r: 0x00,
    g: 0x7c,
    b: 0x80,
};
pub const YELLOW: Colour = Colour {
    r: 0xff,
    g: 0xef,
    b: 0x00,
};
pub const ORANGE: Colour = Colour {
    r: 0xff,
    g: 0xa4,
    b: 0x00,
};
pub const GRADIENT: [Colour; 7] = [
    RED, VIOLET, BLUE, TEAL, GREEN, YELLOW, ORANGE,
];

#[cfg_attr(feature = "dbus", derive(Type, Value, OwnedValue))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AuraDeviceType {
    /// Most new laptops
    #[default]
    LaptopKeyboard2021 = 0,
    LaptopKeyboardPre2021 = 1,
    LaptopKeyboardTuf = 2,
    ScsiExtDisk = 3,
    Ally = 4,
    AnimeOrSlash = 5,
    /// GZ302EA 18c6 rear-window Aura controller, separate from its keyboard.
    RearGlow = 6,
    Unknown = 255,
}

impl AuraDeviceType {
    /// Resolve roles that depend on both the laptop model and USB product ID.
    pub fn for_product_and_board(product_id: &str, board_name: &str) -> Self {
        if board_name == "GZ302EA"
            && product_id
                .trim_start_matches("0x")
                .eq_ignore_ascii_case("18c6")
        {
            return Self::RearGlow;
        }
        Self::from(product_id)
    }

    pub fn is_old_laptop(&self) -> bool {
        *self == Self::LaptopKeyboardPre2021
    }

    pub fn is_tuf_laptop(&self) -> bool {
        *self == Self::LaptopKeyboardTuf
    }

    pub fn is_new_laptop(&self) -> bool {
        *self == Self::LaptopKeyboard2021
    }
}

impl From<&str> for AuraDeviceType {
    fn from(s: &str) -> Self {
        match s.to_lowercase().trim_start_matches("0x") {
            "tuf" => AuraDeviceType::LaptopKeyboardTuf,
            "1932" => AuraDeviceType::ScsiExtDisk,
            "1866" | "18c6" | "1869" | "1854" => Self::LaptopKeyboardPre2021,
            "1abe" | "1b4c" => Self::Ally,
            "19b3" | "193b" => Self::AnimeOrSlash,
            "19b6" | "1a30" | "1ce6" => Self::LaptopKeyboard2021,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod device_role_tests {
    use super::AuraDeviceType;

    #[test]
    fn gz302_rear_and_keyboard_are_distinct() {
        assert_eq!(
            AuraDeviceType::for_product_and_board("18c6", "GZ302EA"),
            AuraDeviceType::RearGlow
        );
        assert_eq!(
            AuraDeviceType::for_product_and_board("1a30", "GZ302EA"),
            AuraDeviceType::LaptopKeyboard2021
        );
        assert_eq!(
            AuraDeviceType::for_product_and_board("18c6", "G533QS"),
            AuraDeviceType::LaptopKeyboardPre2021
        );
        assert_eq!(
            AuraDeviceType::for_product_and_board("18c6", "GZ302XX"),
            AuraDeviceType::LaptopKeyboardPre2021
        );
    }
}

/// The powerr zones this laptop supports
#[cfg_attr(
    feature = "dbus",
    derive(Type, Value, OwnedValue),
    zvariant(signature = "u")
)]
#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Default, Copy, Clone)]
pub enum PowerZones {
    /// The logo on some laptop lids
    Logo = 0,
    /// The full keyboard (not zones)
    #[default]
    Keyboard = 1,
    /// The lightbar, typically on the front of the laptop
    Lightbar = 2,
    /// The leds that may be placed around the edge of the laptop lid
    Lid = 3,
    /// The led strip on the rear of some laptops
    RearGlow = 4,
    /// Exists for the older 0x1866 models
    KeyboardAndLightbar = 5,
    /// Ally specific for creating correct packet
    Ally = 6,
    None = 255,
}
