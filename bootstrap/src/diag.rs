#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Pos {
    pub file: u32,
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Debug)]
pub struct Diag {
    pub pos: Pos,
    pub kind: &'static str, // "error" or "hole"
    pub msg: String,
    pub notes: Vec<String>,
}

impl Diag {
    pub fn new(pos: Pos, msg: &str) -> Diag {
        Diag { pos, kind: "error", msg: msg.to_string(), notes: Vec::new() }
    }

    pub fn note(mut self, n: &str) -> Diag {
        self.notes.push(n.to_string());
        self
    }
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

pub fn render(d: &Diag, files: &[String], json: bool) -> String {
    let file = files.get(d.pos.file as usize).map(|s| s.as_str()).unwrap_or("?");
    if json {
        let notes: Vec<String> = d.notes.iter().map(|n| json_str(n)).collect();
        format!(
            "{{\"kind\":{},\"file\":{},\"line\":{},\"col\":{},\"message\":{},\"notes\":[{}]}}\n",
            json_str(d.kind),
            json_str(file),
            d.pos.line,
            d.pos.col,
            json_str(&d.msg),
            notes.join(",")
        )
    } else {
        let mut s = format!("{}:{}:{}: {}: {}\n", file, d.pos.line, d.pos.col, d.kind, d.msg);
        for n in &d.notes {
            s.push_str(&format!("    {}\n", n));
        }
        s
    }
}
