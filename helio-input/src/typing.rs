//! Typing text by the keys of a layout (pure): which key types each
//! keysym, and whether with Shift. The X11 sink reads the layout from the X
//! server, KWin's from the keymap it sends; both then press the key that
//! types a character, as a keyboard would, rather than ask the display
//! server to type the keysym.

use std::collections::HashMap;

/// Shift_L.
pub const SHIFT_L: u32 = 0xFFE1;

/// Keysym -> the key that types it (the first key found wins: the main
/// block comes first) and whether it takes Shift. Only the first two
/// levels: the others need modifiers a client may interpret differently.
pub fn typing_map(min: u8, per: u8, keysyms: &[u32], spare: &[u8]) -> HashMap<u32, (u8, bool)> {
    let mut map = HashMap::new();
    for (i, syms) in keysyms.chunks(per.max(1) as usize).enumerate() {
        let kc = min as usize + i;
        let Ok(kc) = u8::try_from(kc) else { break };
        if spare.contains(&kc) {
            continue;
        }
        for (level, &sym) in syms.iter().take(2).enumerate() {
            if sym != 0 {
                map.entry(sym).or_insert((kc, level == 1));
            }
        }
        // A key with only a lowercase letter types its capital with Shift.
        if let (Some(&lower), true) = (syms.first(), syms.get(1).is_none_or(|&s| s == 0)) {
            if (0x61..=0x7A).contains(&lower) {
                map.entry(lower - 0x20).or_insert((kc, true));
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_types_what_it_has() {
        // min key code 10; two levels each: (1, !), (a, A), (b, nothing), Shift_L.
        let keysyms = [0x31, 0x21, 0x61, 0x41, 0x62, 0, SHIFT_L, 0, 0, 0];
        let map = typing_map(10, 2, &keysyms, &[14]);
        assert_eq!(map.get(&0x31), Some(&(10, false)));
        assert_eq!(map.get(&0x21), Some(&(10, true)));
        assert_eq!(map.get(&0x41), Some(&(11, true)));
        assert_eq!(
            map.get(&0x42),
            Some(&(12, true)),
            "B from the b key with Shift"
        );
        assert_eq!(map.get(&SHIFT_L), Some(&(13, false)));
        assert_eq!(map.get(&0xE9), None, "é is not on this layout");
    }
}
