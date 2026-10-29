//! Headless tools: convert between formats, validate files, list templates.
use mm_core::{outline, templates, Document};
use mm_io::{csv_io, json, native, Report};
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "\
mm-cli — mind map conversion tools

USAGE:
  mm-cli convert <in> <out> [--delimiter ,|;|tab] [--flat] [--bom] [--no-guard] [--layout adjacency|path|level]
  mm-cli validate <file>
  mm-cli templates
  mm-cli new <template-name> <out.mm.json>

Formats are chosen by extension: .mm.json/.json (native, interchange or generic JSON),
.csv/.tsv, .md/.txt (indented outline). Use '-' for stdin/stdout (format by --from/--to ext).
";

fn ext(p: &str) -> String {
    let lower = p.to_ascii_lowercase();
    if lower.ends_with(".mm.json") {
        "mm.json".into()
    } else {
        Path::new(&lower).extension().and_then(|e| e.to_str()).unwrap_or("").to_string()
    }
}

fn load(path: &str, flags: &Flags, report: &mut Report) -> Result<Document, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let e = ext(path);
    match e.as_str() {
        "csv" | "tsv" => {
            let o = csv_io::ImportOptions {
                delimiter: flags.delimiter.or(if e == "tsv" { Some(b'\t') } else { None }),
                layout: flags.layout,
                trim: true,
                ..Default::default()
            };
            csv_io::import(&bytes, &o, report).map_err(|e| e.to_string())
        }
        "md" | "txt" => {
            let text = mm_io::decode_text(&bytes, report);
            Ok(Document::from_subtree(&outline::parse_outline_single(&text, "Imported")))
        }
        _ => {
            let text = mm_io::decode_text(&bytes, report);
            json::import(&text, &Default::default(), report).map_err(|e| e.to_string())
        }
    }
}

#[derive(Default)]
struct Flags {
    delimiter: Option<u8>,
    flat: bool,
    bom: bool,
    no_guard: bool,
    layout: Option<csv_io::Layout>,
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pos = Vec::new();
    let mut f = Flags::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--flat" => f.flat = true,
            "--bom" => f.bom = true,
            "--no-guard" => f.no_guard = true,
            "--delimiter" => {
                f.delimiter = Some(match it.next().map(String::as_str) {
                    Some(",") => b',',
                    Some(";") => b';',
                    Some("tab") | Some("\\t") => b'\t',
                    other => return Err(format!("bad --delimiter {other:?}")),
                })
            }
            "--layout" => {
                f.layout = Some(match it.next().map(String::as_str) {
                    Some("adjacency") => csv_io::Layout::Adjacency,
                    Some("path") => csv_io::Layout::Path,
                    Some("level") => csv_io::Layout::Level,
                    other => return Err(format!("bad --layout {other:?}")),
                })
            }
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            _ => pos.push(a.clone()),
        }
    }
    match pos.first().map(String::as_str) {
        Some("convert") if pos.len() == 3 => {
            let mut rep = Report::default();
            let doc = load(&pos[1], &f, &mut rep)?;
            let out = &pos[2];
            let bytes: Vec<u8> = match ext(out).as_str() {
                "csv" | "tsv" => csv_io::export(
                    &doc,
                    &csv_io::ExportOptions {
                        delimiter: f.delimiter.unwrap_or(if ext(out) == "tsv" { b'\t' } else { b',' }),
                        bom: f.bom,
                        injection_guard: !f.no_guard,
                        ..Default::default()
                    },
                )
                .map_err(|e| e.to_string())?,
                "md" | "txt" => outline::to_outline(&doc, doc.root()).into_bytes(),
                "mm.json" => native::to_string(&doc, None).map_err(|e| e.to_string())?.into_bytes(),
                _ => json::export(
                    &doc,
                    &json::ExportOptions { shape: if f.flat { json::Shape2::Flat } else { json::Shape2::Nested }, ..Default::default() },
                )
                .map_err(|e| e.to_string())?
                .into_bytes(),
            };
            mm_io::write_atomic(Path::new(out), &bytes).map_err(|e| e.to_string())?;
            for w in &rep.warnings {
                eprintln!("warning: {w}");
            }
            eprintln!("{} nodes -> {out}", doc.len());
            Ok(())
        }
        Some("validate") if pos.len() == 2 => {
            let mut rep = Report::default();
            let doc = load(&pos[1], &f, &mut rep)?;
            doc.check_invariants()?;
            for w in &rep.warnings {
                eprintln!("warning: {w}");
            }
            println!("ok: {} nodes, mode {:?}", doc.len(), doc.mode);
            Ok(())
        }
        Some("templates") => {
            for t in templates::TEMPLATES {
                println!("{}", t.name);
            }
            Ok(())
        }
        Some("new") if pos.len() == 3 => {
            let t = templates::TEMPLATES
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(&pos[1]))
                .ok_or_else(|| format!("unknown template {:?} (see `mm-cli templates`)", pos[1]))?;
            native::save_path(Path::new(&pos[2]), &t.build(), None).map_err(|e| e.to_string())
        }
        _ => {
            print!("{USAGE}");
            Err("bad arguments".into())
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
