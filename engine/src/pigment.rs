//! Kubelka-Munk pigment model shared by the brush, the palette and the UI.
//!
//! A pigment is stored as absorption `K` (rgb) and scattering `S` per unit of
//! concentration. Mixing pigments is a concentration-weighted sum of K and S
//! (Duncan's law), which is what makes blue + yellow come out green instead of
//! the grey an RGB average gives.

pub type Pigment = [f32; 4];

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// Scattering of one unit of pigment. Watercolour pigments are mostly
/// transparent, so S is small and colour comes from absorption.
pub const UNIT_S: f32 = 0.05;
/// Reflectance of the bare paper the colours are referenced against.
pub const PAPER: f32 = 0.9;

/// Pigment that, laid down at unit concentration on white paper, shows `rgb`.
/// Thinner layers give lighter tints, thicker ones go darker, like real paint.
pub fn from_srgb(rgb: [f32; 3]) -> Pigment {
    let mut p = [0.0, 0.0, 0.0, UNIT_S];
    for i in 0..3 {
        let r = srgb_to_linear(rgb[i]).clamp(0.002, PAPER);
        // Film reflectance falls monotonically with K: bisect for the target.
        let (mut lo, mut hi) = (0.0f32, 30.0f32);
        for _ in 0..40 {
            let mid = 0.5 * (lo + hi);
            if film_channel(mid, UNIT_S, PAPER) > r { lo = mid } else { hi = mid }
        }
        p[i] = 0.5 * (lo + hi);
    }
    p
}

/// The thinnest paint a pick takes up, as a fraction of unit concentration.
pub const MIN_PICK: f32 = 0.03;

/// Paint picked up from where it lies (the tray or the canvas), as thick as it
/// lies: thinner than unit concentration stays a thinned paint, anything thicker
/// is taken at unit concentration, and barely tinted water isn't picked at all.
pub fn picked(mean: Pigment) -> Option<Pigment> {
    if mean[3] < UNIT_S * MIN_PICK {
        return None;
    }
    Some(if mean[3] > UNIT_S { normalize(mean) } else { mean })
}

/// What a pigment looks like on white paper, as sRGB: thinned pigment (below
/// unit concentration) as the lighter tint it lays down, anything thicker at
/// unit concentration.
pub fn to_srgb(p: Pigment) -> [f32; 3] {
    let p = if p[3] > UNIT_S { normalize(p) } else { p };
    film_over(p, [PAPER; 3]).map(linear_to_srgb)
}

/// How concentrated a pigment is, 0 (just water) to 1 (unit concentration, as
/// [`from_srgb`] makes it). Thicker mixtures count as 1.
pub fn strength(p: Pigment) -> f32 {
    (p[3] / UNIT_S).clamp(0.0, 1.0)
}

/// Rescales a mixture to unit concentration.
pub fn normalize(p: Pigment) -> Pigment {
    let s = p[3];
    if s <= 1e-6 {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let k = UNIT_S / s;
    [p[0] * k, p[1] * k, p[2] * k, UNIT_S]
}

pub fn mix(a: Pigment, b: Pigment, t: f32) -> Pigment {
    let mut o = [0.0; 4];
    for i in 0..4 {
        o[i] = a[i] * (1.0 - t) + b[i] * t;
    }
    o
}

/// Reflectance (linear) of a pigment film of summed K, S lying on a surface of reflectance `under`.
pub fn film_over(p: Pigment, under: [f32; 3]) -> [f32; 3] {
    [film_channel(p[0], p[3], under[0]), film_channel(p[1], p[3], under[1]), film_channel(p[2], p[3], under[2])]
}

/// Kubelka-Munk reflectance of one channel of a film over a background.
pub fn film_channel(k: f32, s: f32, under: f32) -> f32 {
    let s = s.max(1e-4);
    let a = 1.0 + k / s;
    let b = (a * a - 1.0).max(0.0).sqrt().max(1e-3);
    let bs = (b * s).min(20.0);
    let (sh, ch) = (bs.sinh(), bs.cosh());
    let d = a * sh + b * ch;
    let (r, t) = (sh / d, b / d);
    r + t * t * under / (1.0 - r * under).max(1e-4)
}

/// A small set of real watercolour pigments, approximated by their masstones.
pub const PRESETS: &[(&str, [f32; 3])] = &[
    ("Hansa Yellow", [0.98, 0.80, 0.08]),
    ("New Gamboge", [0.96, 0.60, 0.05]),
    ("Pyrrol Scarlet", [0.86, 0.16, 0.10]),
    ("Quinacridone Rose", [0.80, 0.12, 0.38]),
    ("Ultramarine", [0.16, 0.20, 0.62]),
    ("Phthalo Blue", [0.03, 0.22, 0.50]),
    ("Phthalo Green", [0.00, 0.40, 0.33]),
    ("Burnt Sienna", [0.55, 0.24, 0.12]),
    ("Raw Umber", [0.40, 0.31, 0.20]),
    ("Payne's Grey", [0.22, 0.25, 0.30]),
    ("Indigo", [0.15, 0.16, 0.26]),
    ("Lamp Black", [0.07, 0.07, 0.07]),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_roundtrip() {
        let c = [0.8, 0.3, 0.1];
        let back = to_srgb(from_srgb(c));
        for i in 0..3 {
            assert!((back[i] - c[i]).abs() < 0.03, "{back:?}");
        }
    }

    #[test]
    fn thinned_pigment_is_a_lighter_tint() {
        let full = from_srgb([0.16, 0.20, 0.62]);
        let thin = full.map(|v| v * 0.3);
        let (a, b) = (to_srgb(full), to_srgb(thin));
        assert!((0..3).all(|i| b[i] > a[i]), "{a:?} {b:?}");
        assert!((strength(thin) - 0.3).abs() < 1e-4);
        let thick = to_srgb(full.map(|v| v * 3.0));
        assert!((0..3).all(|i| (thick[i] - a[i]).abs() < 1e-5), "{thick:?}");
    }

    #[test]
    fn blue_and_yellow_make_green() {
        let m = to_srgb(mix(from_srgb([0.1, 0.25, 0.75]), from_srgb([0.95, 0.85, 0.1]), 0.5));
        assert!(m[1] > m[0] && m[1] > m[2], "{m:?}");
    }
}
