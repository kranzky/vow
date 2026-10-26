mod ast;
mod check;
mod codegen;
mod diag;
mod lexer;
mod parser;
mod pretty;
mod verify;

use ast::Item;
use diag::{Diag, Pos};
use std::path::{Path, PathBuf};
use std::process::exit;

const USAGE: &str = "usage: vowc <command> <file.vow> [options]

commands:
  check   type-check and verify
  build   check, verify and compile to an executable (-o <path>)
  run     build and run (remaining arguments after -- go to the program)
  emit-c  print the generated C
  hash    print each function's body hash and verification key

options:
  --json        machine-readable diagnostics (one JSON object per line)
  --no-verify   skip the Z3 verifier (unsafe: contracts are not checked)
  --no-cache    ignore and do not update the verification cache
  -o <path>     output executable for build
";

pub struct Source {
    pub files: Vec<String>,
    pub items: Vec<Item>,
}

fn load(root: &str) -> Result<Source, (Vec<String>, Vec<Diag>)> {
    let mut files: Vec<String> = Vec::new();
    let mut canon: Vec<PathBuf> = Vec::new();
    let mut items = Vec::new();
    let mut next_id = 0usize;
    let mut queue: Vec<(PathBuf, Pos)> = vec![(PathBuf::from(root), Pos::default())];
    let mut errs = Vec::new();
    while let Some((path, from)) = queue.pop() {
        let c = match std::fs::canonicalize(&path) {
            Ok(c) => c,
            Err(_) => {
                files.push(path.display().to_string());
                errs.push(Diag::new(from, &format!("cannot open '{}'", path.display())));
                continue;
            }
        };
        if canon.contains(&c) {
            continue;
        }
        canon.push(c.clone());
        let file_id = files.len() as u32;
        files.push(path.display().to_string());
        let src = match std::fs::read_to_string(&c) {
            Ok(s) => s,
            Err(_) => {
                errs.push(Diag::new(from, &format!("cannot read '{}'", path.display())));
                continue;
            }
        };
        let toks = match lexer::lex(&src, file_id) {
            Ok(t) => t,
            Err(d) => {
                errs.push(d);
                continue;
            }
        };
        let mut p = parser::Parser::new(&toks, next_id);
        match p.parse_file() {
            Ok(parsed) => {
                next_id = p.next_id;
                let dir = c.parent().map(|d| d.to_path_buf()).unwrap_or_default();
                // Push in reverse so imports are processed in source order.
                for (imp, pos) in parsed.imports.iter().rev() {
                    queue.push((dir.join(imp), *pos));
                }
                items.extend(parsed.items);
            }
            Err(d) => errs.push(d),
        }
    }
    if errs.is_empty() {
        Ok(Source { files, items })
    } else {
        Err((files, errs))
    }
}

fn report(diags: &[Diag], files: &[String], json: bool) {
    for d in diags {
        eprint!("{}", diag::render(d, files, json));
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.len() < 2 {
        eprint!("{}", USAGE);
        exit(2);
    }
    let cmd = argv[0].as_str();
    let file = argv[1].clone();
    let mut json = false;
    let mut no_verify = false;
    let mut out: Option<String> = None;
    let mut i = 2;
    while i < argv.len() {
        match argv[i].as_str() {
            "--json" => json = true,
            "--no-verify" => no_verify = true,
            "-o" if i + 1 < argv.len() => {
                out = Some(argv[i + 1].clone());
                i += 1;
            }
            "--" => break,
            other => {
                eprintln!("unknown option '{}'\n\n{}", other, USAGE);
                exit(2);
            }
        }
        i += 1;
    }
    let run_args: Vec<String> = match argv.iter().position(|a| a == "--") {
        Some(p) => argv[p + 1..].to_vec(),
        None => Vec::new(),
    };
    let src = match load(&file) {
        Ok(s) => s,
        Err((files, errs)) => {
            report(&errs, &files, json);
            exit(1);
        }
    };
    let (info, errs) = check::check_program(&src.items);
    if !errs.is_empty() {
        report(&errs, &src.files, json);
        exit(1);
    }
    // Holes and verification. Holes are reported together with the facts the verifier
    // knows at that point, so the verifier runs even when there are holes.
    let mut verified = false;
    if !no_verify || !info.holes.is_empty() {
        let mut v = verify::Verifier::new(&info);
        let mut vcs = Vec::new();
        for it in &src.items {
            if let Item::Fn(f) = it {
                vcs.push(v.function(f));
            }
        }
        if !info.holes.is_empty() {
            let mut diags = Vec::new();
            for h in &info.holes {
                let mut d = Diag::new(h.pos, &format!("hole ?{} needs a value of type {}", h.name, h.expected.show()));
                d.kind = "hole";
                let locals: Vec<String> = h.locals.iter().map(|(n, t)| format!("{}: {}", n, t.show())).collect();
                d = d.note(&format!("in scope: {}", if locals.is_empty() { "(nothing)".into() } else { locals.join(", ") }));
                if let Some(facts) = v.hole_facts.get(&h.id) {
                    if !facts.is_empty() {
                        d = d.note(&format!("known facts: {}", facts.join("; ")));
                    }
                }
                diags.push(d);
            }
            report(&diags, &src.files, json);
            exit(1);
        }
        let pre: Vec<String> = v.preamble.iter().cloned().collect();
        let pre = order_preamble(pre);
        match verify::discharge(&pre, &vcs) {
            Err(e) => {
                eprintln!("vowc: {}", e);
                exit(1);
            }
            Ok(fails) => {
                if !fails.is_empty() {
                    let diags: Vec<Diag> = fails.iter().map(verify::failure_diag).collect();
                    report(&diags, &src.files, json);
                    exit(1);
                }
            }
        }
        verified = !no_verify;
    }
    let c = codegen::generate(&src.items, &info, &src.files, verified);
    match cmd {
        "check" => println!("ok"),
        "emit-c" => print!("{}", c),
        "build" | "run" => {
            let exe = match (&out, cmd) {
                (Some(o), _) => PathBuf::from(o),
                (None, "build") => PathBuf::from(Path::new(&file).file_stem().unwrap()),
                _ => std::env::temp_dir().join(format!("vow-run-{}", std::process::id())),
            };
            if let Err(e) = compile_c(&c, &exe) {
                eprintln!("vowc: {}", e);
                exit(1);
            }
            if cmd == "run" {
                let status = std::process::Command::new(&exe).args(&run_args).status();
                let _ = std::fs::remove_file(&exe);
                match status {
                    Ok(s) => exit(s.code().unwrap_or(1)),
                    Err(e) => {
                        eprintln!("vowc: cannot run program: {}", e);
                        exit(1);
                    }
                }
            }
        }
        _ => {
            eprint!("unknown command '{}'\n\n{}", cmd, USAGE);
            exit(2);
        }
    }
}

/// The sort declaration must come first; declarations before assertions.
fn order_preamble(mut pre: Vec<String>) -> Vec<String> {
    pre.sort_by_key(|l| {
        if l.starts_with("(declare-sort") {
            0
        } else if l.starts_with("(declare") {
            1
        } else {
            2
        }
    });
    pre
}

const RUNTIME: &str = include_str!("../../runtime/vow_rt.h");

fn compile_c(c: &str, exe: &Path) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("vowc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("vow_rt.h"), RUNTIME).map_err(|e| e.to_string())?;
    let cfile = dir.join("program.c");
    std::fs::write(&cfile, c).map_err(|e| e.to_string())?;
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let res = std::process::Command::new(&cc)
        .args(["-O2", "-std=c11", "-w", "-o"])
        .arg(exe)
        .arg(&cfile)
        .status()
        .map_err(|e| format!("cannot run C compiler '{}': {}", cc, e))?;
    let _ = std::fs::remove_dir_all(&dir);
    if res.success() {
        Ok(())
    } else {
        Err(format!("C compiler '{}' failed", cc))
    }
}
