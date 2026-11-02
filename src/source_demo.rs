//! Offline local parser check; intentionally prints no source contents or identities.
use niccolo_evidence::source_import::{SourceFormat, parse_source};
fn main() {
    let mut args = std::env::args_os().skip(1);
    let format = args.next().unwrap_or_default();
    let path = args.next().unwrap_or_default();
    if format.is_empty() || path.is_empty() || args.next().is_some() {
        eprintln!("usage: niccolo-source-demo (eml|whatsapp) SOURCE_FILE");
        std::process::exit(2);
    }
    let format = match format.to_str() {
        Some("eml") => SourceFormat::Eml,
        Some("whatsapp") => SourceFormat::WhatsappExport,
        _ => {
            eprintln!("unsupported format");
            std::process::exit(2)
        }
    };
    let raw = match std::fs::read(&path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("could not read source: {e}");
            std::process::exit(1)
        }
    };
    match parse_source(format, &raw) {
        Ok(r) => {
            println!(
                "sha256={} bytes={} fragments={}",
                r.sha256,
                r.byte_length,
                r.fragments.len()
            );
            for f in &r.fragments {
                println!(
                    "fragment={} start={} end={}",
                    f.ordinal, f.byte_start, f.byte_end
                );
            }
        }
        Err(e) => {
            eprintln!("source rejected: {e}");
            std::process::exit(1)
        }
    }
}
