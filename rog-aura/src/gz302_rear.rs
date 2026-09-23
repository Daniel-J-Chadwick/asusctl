//! GZ302EA rear-window lighting on USB 0b05:18c6.
//!
//! Interface 0 carries ASUS Aura output reports that wake and power the rear
//! controller. Interface 1 is an eleven-lamp HID LampArray whose feature
//! reports control the visible RGB colour. The 1a30 keyboard is separate.

use crate::{Colour, LedBrightness};

pub const AURA_REPORT_LEN: usize = 64;
pub const LAMP_COUNT: usize = 11;
pub const MULTI_REPORT_LEN: usize = 51;

fn aura_report(payload: &[u8]) -> [u8; AURA_REPORT_LEN] {
    let mut report = [0; AURA_REPORT_LEN];
    report[..payload.len()].copy_from_slice(payload);
    report
}

/// z13ctl's documented wake, identity, configuration, power and brightness
/// sequence. Its zone-1 B3 effect is deliberately omitted: local green/blue
/// tests showed it wakes the rear light but does not set the visible colour.
pub fn wake_reports(brightness: LedBrightness) -> [[u8; AURA_REPORT_LEN]; 6] {
    let power = if brightness == LedBrightness::Off {
        aura_report(&[
            0x5d, 0xbd, 0x01, 0, 0, 0, 0, 0xff,
        ])
    } else {
        aura_report(&[
            0x5d, 0xbd, 0x01, 0xff, 0x1f, 0xff, 0xff, 0xff,
        ])
    };
    [
        aura_report(&[
            0x5d, 0xb9,
        ]),
        aura_report(b"]ASUS Tech.Inc."),
        aura_report(&[
            0x5d, 0x05, 0x20, 0x31, 0, 0x1a,
        ]),
        aura_report(&[
            0x5d, 0xc0, 0x03, 0x01,
        ]),
        power,
        aura_report(&[
            0x5d, 0xba, 0xc5, 0xc4, brightness as u8,
        ]),
    ]
}

/// Standard HID LampArrayControl feature report, including report ID 6.
pub fn lamp_control(autonomous: bool) -> [u8; 2] {
    [
        6,
        u8::from(autonomous),
    ]
}

/// Standard HID LampMultiUpdate report 4 in two batches of eight and three.
/// GZ302EA's attributes report 11 programmable lamps and one nonzero
/// intensity level, so every active lamp uses intensity 1.
pub fn static_colour_reports(colour: Colour) -> [[u8; MULTI_REPORT_LEN]; 2] {
    let mut reports = [[0; MULTI_REPORT_LEN]; 2];
    for (batch, start) in [
        0usize, 8,
    ]
    .into_iter()
    .enumerate()
    {
        let count = (LAMP_COUNT - start).min(8);
        let report = &mut reports[batch];
        report[0] = 4;
        report[1] = count as u8;
        report[2] = u8::from(start + count == LAMP_COUNT);
        for slot in 0..count {
            let lamp_id = (start + slot) as u16;
            report[3 + slot * 2..5 + slot * 2].copy_from_slice(&lamp_id.to_le_bytes());
            report[19 + slot * 4..23 + slot * 4].copy_from_slice(&[
                colour.r, colour.g, colour.b, 1,
            ]);
        }
    }
    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_reports_match_source_documented_packets() {
        let expected = [
            &[
                0x5d, 0xb9,
            ][..],
            &b"]ASUS Tech.Inc."[..],
            &[
                0x5d, 0x05, 0x20, 0x31, 0, 0x1a,
            ],
            &[
                0x5d, 0xc0, 0x03, 0x01,
            ],
            &[
                0x5d, 0xbd, 0x01, 0xff, 0x1f, 0xff, 0xff, 0xff,
            ],
            &[
                0x5d, 0xba, 0xc5, 0xc4, 3,
            ],
        ];
        assert_eq!(wake_reports(LedBrightness::High), expected.map(aura_report));
        let off = wake_reports(LedBrightness::Off);
        assert_eq!(
            off[4],
            aura_report(&[
                0x5d, 0xbd, 0x01, 0, 0, 0, 0, 0xff
            ])
        );
        assert_eq!(
            off[5],
            aura_report(&[
                0x5d, 0xba, 0xc5, 0xc4, 0
            ])
        );
    }

    #[test]
    fn green_matches_verified_complete_lamparray_packets() {
        let [
            first,
            second,
        ] = static_colour_reports(Colour { r: 0, g: 255, b: 0 });
        let expected_first = [
            &[
                4, 8, 0, 0, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0,
            ][..],
            &[
                0, 255, 0, 1,
            ]
            .repeat(8),
        ]
        .concat();
        let expected_second = [
            &[
                4, 3, 1, 8, 0, 9, 0, 10, 0,
            ][..],
            &[0; 10],
            &[
                0, 255, 0, 1,
            ]
            .repeat(3),
            &[0; 20],
        ]
        .concat();
        assert_eq!(first.as_slice(), expected_first.as_slice());
        assert_eq!(second.as_slice(), expected_second.as_slice());
        assert_eq!(lamp_control(true), [6, 1]);
        assert_eq!(lamp_control(false), [6, 0]);
    }
}
