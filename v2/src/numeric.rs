//! ECMAScript StringNumericLiteral, parseInt and Number#toFixed(3) for binary64.
use num_bigint::BigUint;
use num_traits::{One, ToPrimitive, Zero};

fn whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

pub fn js_string(value: f64) -> String {
    ryu_js::Buffer::new().format(value).to_owned()
}

pub fn parse_int(text: &str) -> f64 {
    let mut text = text.trim_start_matches(whitespace);
    let negative = text.starts_with('-');
    if text.starts_with(['-', '+']) {
        text = &text[1..];
    }
    let radix = if text.starts_with("0x") || text.starts_with("0X") {
        text = &text[2..];
        16
    } else {
        10
    };
    let end = text
        .bytes()
        .take_while(|b| (*b as char).is_digit(radix))
        .count();
    if end == 0 {
        return f64::NAN;
    }
    let digits = &text[..end];
    let number = if radix == 10 {
        digits.parse::<f64>().unwrap_or(f64::INFINITY)
    } else {
        BigUint::parse_bytes(digits.as_bytes(), radix)
            .unwrap()
            .to_f64()
            .unwrap_or(f64::INFINITY)
    };
    if negative {
        -number
    } else {
        number
    }
}

pub fn number(text: &str) -> f64 {
    let text = text.trim_matches(whitespace);
    if text.is_empty() {
        return 0.0;
    }
    if matches!(text, "Infinity" | "+Infinity") {
        return f64::INFINITY;
    }
    if text == "-Infinity" {
        return f64::NEG_INFINITY;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = text.strip_prefix(prefix) {
            if digits.is_empty() || !digits.bytes().all(|b| (b as char).is_digit(radix)) {
                return f64::NAN;
            }
            return BigUint::parse_bytes(digits.as_bytes(), radix)
                .unwrap()
                .to_f64()
                .unwrap_or(f64::INFINITY);
        }
    }
    // Rust accepts spellings such as `inf`; ECMAScript accepts decimal grammar only.
    let bytes = text.as_bytes();
    let mut i = usize::from(bytes.first().is_some_and(|b| matches!(b, b'+' | b'-')));
    let start = i;
    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    let mut count = i - start;
    if bytes.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        count += i - start;
    }
    if count == 0 {
        return f64::NAN;
    }
    if bytes.get(i).is_some_and(|b| matches!(b, b'e' | b'E')) {
        i += 1;
        if bytes.get(i).is_some_and(|b| matches!(b, b'+' | b'-')) {
            i += 1;
        }
        let start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return f64::NAN;
        }
    }
    if i != bytes.len() {
        return f64::NAN;
    }
    text.parse().unwrap_or(f64::NAN)
}

pub fn to_fixed_3(value: f64) -> String {
    if !value.is_finite() || value.abs() >= 1e21 {
        return js_string(value);
    }
    // ECMAScript uses x < 0: -0 itself has no sign, tiny negative values do.
    let negative = value < 0.0;
    let bits = value.abs().to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32;
    let fraction = bits & ((1_u64 << 52) - 1);
    let significand = if exponent == 0 {
        fraction
    } else {
        fraction | (1 << 52)
    };
    let shift = if exponent == 0 {
        -1074
    } else {
        exponent - 1023 - 52
    };
    let mut scaled = BigUint::from(significand) * BigUint::from(1000_u32);
    if shift >= 0 {
        scaled <<= shift as usize;
    } else {
        let shift = (-shift) as usize;
        let denominator = BigUint::one() << shift;
        let remainder = &scaled % &denominator;
        scaled >>= shift;
        if remainder * BigUint::from(2_u8) >= denominator {
            scaled += BigUint::one();
        }
    }
    let mut digits = if scaled.is_zero() {
        "0".to_string()
    } else {
        scaled.to_str_radix(10)
    };
    while digits.len() < 4 {
        digits.insert(0, '0');
    }
    digits.insert(digits.len() - 3, '.');
    if negative {
        digits.insert(0, '-');
    }
    digits
}
