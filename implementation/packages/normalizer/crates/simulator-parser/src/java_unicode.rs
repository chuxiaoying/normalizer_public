//! Java's Unicode translation precedes comments, line terminators and tokens.
//! Keep physical offsets for diagnostics; the translated source is not a rewrite
//! by the business preprocessing layer.
pub(super) struct Translation {
    pub(super) text: String,
    origins: Vec<usize>,
    surrogates: Vec<(usize, u16)>,
}

/// Side data for isolated UTF-16 units. A replacement glyph in translated UTF-8
/// is just a placeholder: actual U+FFFD input has no corresponding side entry.
#[derive(Clone, Copy, Default)]
pub(crate) struct Surrogates<'a> {
    units: &'a [(usize, u16)],
    base: usize,
}

impl Surrogates<'_> {
    pub(crate) fn at(self, offset: usize) -> Option<u16> {
        let key = self.base + offset;
        let index = self.units.binary_search_by_key(&key, |&(pos, _)| pos).ok()?;
        Some(self.units[index].1)
    }

    pub(crate) fn subspan(self, offset: usize) -> Self {
        Self { base: self.base + offset, ..self }
    }
}

pub(super) fn advance_position(b: u8, previous_cr: bool, row: &mut usize, column: &mut usize) -> bool {
    if matches!(b, b'\r' | b'\n') {
        *row += usize::from(!(previous_cr && b == b'\n'));
        *column = 1;
    } else {
        *column += 1;
    }
    b == b'\r'
}

impl Translation {
    pub(super) fn surrogates(&self) -> Surrogates<'_> {
        Surrogates { units: &self.surrogates, base: 0 }
    }

    pub(super) fn raw_offset(&self, line: usize, col: usize) -> usize {
        let (mut row, mut column) = (1, 1);
        let mut previous_cr = false;
        for (index, b) in self.text.bytes().enumerate() {
            if (row, column) == (line, col) && !(previous_cr && b == b'\n') {
                return self.origins[index];
            }
            previous_cr = advance_position(b, previous_cr, &mut row, &mut column);
        }
        self.origins[self.text.len()]
    }
}

pub(super) fn translate(source: &str) -> Result<Option<Translation>, (usize, &'static str)> {
    if !source.contains("\\u") { return Ok(None); }

    let bytes = source.as_bytes();
    let mut units = Vec::new();
    let mut pos = 0;
    let mut odd_slashes = false;
    let mut last_escaped = false;
    let mut translated = false;

    while pos < bytes.len() {
        let start = pos;
        let escape = bytes[pos] == b'\\' && (last_escaped || !odd_slashes)
            && bytes.get(pos + 1) == Some(&b'u');
        if escape {
            pos += 2;
            while bytes.get(pos) == Some(&b'u') { pos += 1; }
            let mut value = 0u16;
            for _ in 0..4 {
                let digit = match bytes.get(pos).copied() {
                    Some(c @ b'0'..=b'9') => c - b'0',
                    Some(c @ b'a'..=b'f') => c - b'a' + 10,
                    Some(c @ b'A'..=b'F') => c - b'A' + 10,
                    _ => return Err((start, "[JAVA-UNICODE-ESCAPE] expected four hexadecimal digits")),
                };
                value = (value << 4) | u16::from(digit);
                pos += 1;
            }
            units.push((value, start));
            odd_slashes = value == u16::from(b'\\') && !odd_slashes;
            last_escaped = true;
            translated = true;
        } else {
            let c = source[pos..].chars().next().expect("nonempty UTF-8 source remainder");
            units.extend(c.encode_utf16(&mut [0; 2]).iter().map(|&u| (u, start)));
            pos += c.len_utf8();
            odd_slashes = c == '\\' && !odd_slashes;
            last_escaped = false;
        }
    }

    if !translated { return Ok(None); }
    let mut text = String::with_capacity(bytes.len());
    let mut origins = Vec::with_capacity(bytes.len() + 1);
    let mut surrogates = Vec::new();
    let mut unit_index = 0;
    for decoded in char::decode_utf16(units.iter().map(|&(u, _)| u)) {
        let origin = units[unit_index].1;
        let c = match decoded {
            Ok(c) => c,
            Err(error) => {
                surrogates.push((text.len(), error.unpaired_surrogate()));
                char::REPLACEMENT_CHARACTER
            }
        };
        unit_index += c.len_utf16();
        text.push(c);
        origins.resize(text.len(), origin);
    }
    origins.push(source.len());
    Ok(Some(Translation { text, origins, surrogates }))
}
