use crate::diag::{Diag, Pos};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TK {
    Ident,
    Keyword,
    Int,
    Str,
    Hole,
    Sym,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: TK,
    pub text: String,
    pub int: i64,
    pub pos: Pos,
}

pub const KEYWORDS: &[&str] = &[
    "fn", "end", "struct", "enum", "type", "let", "var", "set", "return", "if", "then", "elif",
    "else", "while", "for", "in", "do", "match", "case", "invariant", "requires", "ensures", "uses",
    "assert", "check", "break", "continue", "and", "or", "not", "implies", "true", "false", "with",
    "import", "it", "result", "where", "inout", "forall", "exists", "decreases",
];

// Longest symbols first so that maximal munch works by simple prefix testing.
const SYMBOLS: &[&str] = &[
    "->", "..", "==", "!=", "<=", ">=", "++", "(", ")", "[", "]", "{", "}", ",", ":", ".", "=", "<",
    ">", "+", "-", "*", "/", "%",
];

pub fn lex(src: &str, file: u32) -> Result<Vec<Token>, Diag> {
    let b = src.as_bytes();
    let mut toks = Vec::new();
    let mut i = 0usize;
    let mut line = 1u32;
    let mut col = 1u32;
    while i < b.len() {
        let c = b[i];
        let pos = Pos { file, line, col };
        if c == b'\n' {
            i += 1;
            line += 1;
            col = 1;
            continue;
        }
        if c == b' ' || c == b'\t' || c == b'\r' {
            i += 1;
            col += 1;
            continue;
        }
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let text = &src[start..i];
            col += (i - start) as u32;
            let kind = if KEYWORDS.contains(&text) { TK::Keyword } else { TK::Ident };
            toks.push(Token { kind, text: text.to_string(), int: 0, pos });
            continue;
        }
        if c == b'?' {
            let start = i + 1;
            i += 1;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            if i == start {
                return Err(Diag::new(pos, "expected a name after '?' for a typed hole"));
            }
            col += (i - start + 1) as u32;
            toks.push(Token { kind: TK::Hole, text: src[start..i].to_string(), int: 0, pos });
            continue;
        }
        if c.is_ascii_digit() {
            let mut v: i64 = 0;
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
                if b[i] != b'_' {
                    let d = (b[i] - b'0') as i64;
                    v = match v.checked_mul(10).and_then(|x| x.checked_add(d)) {
                        Some(x) => x,
                        None => return Err(Diag::new(pos, "integer literal does not fit in 64 bits")),
                    };
                }
                i += 1;
            }
            col += (i - start) as u32;
            toks.push(Token { kind: TK::Int, text: src[start..i].to_string(), int: v, pos });
            continue;
        }
        if c == b'\'' {
            // Character literal: an Int holding one byte.
            let (v, n) = if i + 1 < b.len() && b[i + 1] == b'\\' && i + 2 < b.len() {
                let e = b[i + 2];
                let v = match e {
                    b'n' => b'\n',
                    b't' => b'\t',
                    b'r' => b'\r',
                    b'0' => 0,
                    b'\\' => b'\\',
                    b'\'' => b'\'',
                    b'"' => b'"',
                    _ => return Err(Diag::new(pos, &format!("unknown escape '\\{}'", e as char))),
                };
                (v, 4)
            } else if i + 1 < b.len() {
                (b[i + 1], 3)
            } else {
                return Err(Diag::new(pos, "unterminated character literal"));
            };
            if i + n - 1 >= b.len() || b[i + n - 1] != b'\'' {
                return Err(Diag::new(pos, "a character literal holds exactly one byte, like 'a' or '\\n'"));
            }
            toks.push(Token { kind: TK::Int, text: src[i..i + n].to_string(), int: v as i64, pos });
            i += n;
            col += n as u32;
            continue;
        }
        if c == b'"' {
            i += 1;
            col += 1;
            let mut s: Vec<u8> = Vec::new();
            loop {
                if i >= b.len() || b[i] == b'\n' {
                    return Err(Diag::new(pos, "unterminated string literal"));
                }
                let d = b[i];
                if d == b'"' {
                    i += 1;
                    col += 1;
                    break;
                }
                if d == b'\\' {
                    if i + 1 >= b.len() {
                        return Err(Diag::new(pos, "unterminated string literal"));
                    }
                    let e = b[i + 1];
                    let r = match e {
                        b'n' => b'\n',
                        b't' => b'\t',
                        b'r' => b'\r',
                        b'0' => 0,
                        b'\\' => b'\\',
                        b'"' => b'"',
                        _ => {
                            let p = Pos { file, line, col };
                            return Err(Diag::new(p, &format!("unknown escape '\\{}'", e as char)));
                        }
                    };
                    s.push(r);
                    i += 2;
                    col += 2;
                    continue;
                }
                s.push(d);
                i += 1;
                col += 1;
            }
            let text = String::from_utf8_lossy(&s).into_owned();
            toks.push(Token { kind: TK::Str, text, int: 0, pos });
            continue;
        }
        let rest = &src[i..];
        match SYMBOLS.iter().find(|s| rest.starts_with(**s)) {
            Some(s) => {
                i += s.len();
                col += s.len() as u32;
                toks.push(Token { kind: TK::Sym, text: s.to_string(), int: 0, pos });
            }
            None => {
                let ch = rest.chars().next().unwrap();
                return Err(Diag::new(pos, &format!("unexpected character '{}'", ch)));
            }
        }
    }
    toks.push(Token { kind: TK::Eof, text: String::new(), int: 0, pos: Pos { file, line, col } });
    Ok(toks)
}
