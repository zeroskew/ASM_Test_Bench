//! Line tokenizer.

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    Num(i64),
    Str(Vec<u8>),
    Comma,
    Colon,
    LBrack,
    RBrack,
    LParen,
    RParen,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Shl,
    Shr,
    Amp,
    Pipe,
    Caret,
    Tilde,
    /// `$` (current address)
    Here,
    /// `$$` (start of the current section)
    SectStart,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    /// Character columns (start, end).
    pub cols: (usize, usize),
}

pub struct LexError {
    pub cols: (usize, usize),
    pub message: String,
}

pub fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '.' || c == '?' || c == '@'
}

pub fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '?' || c == '@' || c == '$' || c == '#'
}

/// Parses a numeric literal: 123, 0x1F, 1Fh, 0b101, 101b, 17o / 17q, 0o17. Underscores allowed.
pub fn parse_number(text: &str) -> Option<i64> {
    let t: String = text.chars().filter(|&c| c != '_').collect::<String>().to_ascii_lowercase();
    let radix = |s: &str, r: u32| u64::from_str_radix(s, r).ok().map(|v| v as i64);
    if t.is_empty() {
        return None;
    }
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0h")) {
        return radix(h, 16);
    }
    if let Some(h) = t.strip_suffix('h') {
        return radix(h, 16);
    }
    if let Some(b) = t.strip_prefix("0b") {
        if let Some(v) = radix(b, 2) {
            return Some(v);
        }
    }
    if let Some(o) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0q")) {
        return radix(o, 8);
    }
    if let Some(o) = t.strip_suffix('o').or_else(|| t.strip_suffix('q')) {
        return radix(o, 8);
    }
    if let Some(b) = t.strip_suffix('b').or_else(|| t.strip_suffix('y')) {
        if let Some(v) = radix(b, 2) {
            return Some(v);
        }
    }
    if let Some(d) = t.strip_suffix('d') {
        return radix(d, 10);
    }
    radix(&t, 10)
}

/// Byte offset where the comment starts (`;` outside a string), or the line length.
pub fn code_end(line: &str) -> usize {
    let mut quote: Option<char> = None;
    for (i, c) in line.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '\'' || c == '"' || c == '`' => quote = Some(c),
            None if c == ';' => return i,
            None => {}
        }
    }
    line.len()
}

pub fn tokenize(line: &str) -> Result<Vec<Token>, LexError> {
    let chars: Vec<char> = line[..code_end(line)].chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let single = match c {
            ',' => Some(Tok::Comma),
            ':' => Some(Tok::Colon),
            '[' => Some(Tok::LBrack),
            ']' => Some(Tok::RBrack),
            '(' => Some(Tok::LParen),
            ')' => Some(Tok::RParen),
            '+' => Some(Tok::Plus),
            '-' => Some(Tok::Minus),
            '*' => Some(Tok::Star),
            '/' => Some(Tok::Slash),
            '%' => Some(Tok::Percent),
            '&' => Some(Tok::Amp),
            '|' => Some(Tok::Pipe),
            '^' => Some(Tok::Caret),
            '~' => Some(Tok::Tilde),
            _ => None,
        };
        if let Some(t) = single {
            out.push(Token { tok: t, cols: (start, start + 1) });
            i += 1;
            continue;
        }
        if (c == '<' || c == '>') && chars.get(i + 1) == Some(&c) {
            out.push(Token { tok: if c == '<' { Tok::Shl } else { Tok::Shr }, cols: (start, start + 2) });
            i += 2;
            continue;
        }
        if c == '$' {
            if chars.get(i + 1) == Some(&'$') {
                out.push(Token { tok: Tok::SectStart, cols: (start, start + 2) });
                i += 2;
                continue;
            }
            // `$name` escapes an identifier in NASM; `$` alone is the current address.
            if chars.get(i + 1).is_some_and(|&n| is_ident_start(n)) {
                i += 1;
                let s = i;
                while i < chars.len() && is_ident_char(chars[i]) {
                    i += 1;
                }
                out.push(Token { tok: Tok::Ident(chars[s..i].iter().collect()), cols: (start, i) });
                continue;
            }
            // `$0F` style hex numbers.
            if chars.get(i + 1).is_some_and(|n| n.is_ascii_digit()) {
                i += 1;
                let s = i;
                while i < chars.len() && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let txt: String = chars[s..i].iter().collect();
                let v = i64::from_str_radix(&txt, 16).map_err(|_| LexError { cols: (start, i), message: format!("invalid number '${txt}'") })?;
                out.push(Token { tok: Tok::Num(v), cols: (start, i) });
                continue;
            }
            out.push(Token { tok: Tok::Here, cols: (start, start + 1) });
            i += 1;
            continue;
        }
        if c == '\'' || c == '"' || c == '`' {
            let mut j = i + 1;
            let mut bytes = Vec::new();
            let mut closed = false;
            while j < chars.len() {
                let d = chars[j];
                if d == c {
                    closed = true;
                    j += 1;
                    break;
                }
                if c == '`' && d == '\\' && j + 1 < chars.len() {
                    j += 1;
                    let e = chars[j];
                    match e {
                        'n' => bytes.push(b'\n'),
                        't' => bytes.push(b'\t'),
                        'r' => bytes.push(b'\r'),
                        '0' => bytes.push(0),
                        '\\' => bytes.push(b'\\'),
                        '`' => bytes.push(b'`'),
                        '\'' => bytes.push(b'\''),
                        '"' => bytes.push(b'"'),
                        'e' => bytes.push(27),
                        other => {
                            let mut b = [0u8; 4];
                            bytes.extend_from_slice(other.encode_utf8(&mut b).as_bytes());
                        }
                    }
                    j += 1;
                    continue;
                }
                let mut b = [0u8; 4];
                bytes.extend_from_slice(d.encode_utf8(&mut b).as_bytes());
                j += 1;
            }
            if !closed {
                return Err(LexError { cols: (start, chars.len()), message: "unterminated string".into() });
            }
            out.push(Token { tok: Tok::Str(bytes), cols: (start, j) });
            i = j;
            continue;
        }
        if c.is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            let txt: String = chars[i..j].iter().collect();
            let v = parse_number(&txt).ok_or_else(|| LexError { cols: (start, j), message: format!("invalid number '{txt}'") })?;
            out.push(Token { tok: Tok::Num(v), cols: (start, j) });
            i = j;
            continue;
        }
        if is_ident_start(c) {
            let mut j = i;
            while j < chars.len() && is_ident_char(chars[j]) {
                j += 1;
            }
            out.push(Token { tok: Tok::Ident(chars[i..j].iter().collect()), cols: (start, j) });
            i = j;
            continue;
        }
        return Err(LexError { cols: (start, start + 1), message: format!("unexpected character '{c}'") });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(parse_number("123"), Some(123));
        assert_eq!(parse_number("0x1F"), Some(31));
        assert_eq!(parse_number("1Fh"), Some(31));
        assert_eq!(parse_number("0FFh"), Some(255));
        assert_eq!(parse_number("0b101"), Some(5));
        assert_eq!(parse_number("101b"), Some(5));
        assert_eq!(parse_number("17o"), Some(15));
        assert_eq!(parse_number("1_000"), Some(1000));
        assert_eq!(parse_number("0bh"), Some(11));
        assert_eq!(parse_number("12x"), None);
    }

    #[test]
    fn comments_and_strings() {
        let t = tokenize("msg db 'a;b', 10 ; comment").ok().unwrap();
        assert_eq!(t.len(), 5);
        assert_eq!(t[2].tok, Tok::Str(b"a;b".to_vec()));
        let t = tokenize("db `hi\\n`").ok().unwrap();
        assert_eq!(t[1].tok, Tok::Str(b"hi\n".to_vec()));
    }
}
