//! Executable structure reports (PE / ELF / Mach-O).
//!
//! Parses an executable with [`goblin`] — a pure-Rust, fuzz-tested parser that
//! tolerates truncated and adversarial inputs without panicking, which is exactly
//! what you want when poking at hostile binaries. Produces a plain-text report
//! (headers, sections with per-section Shannon entropy, and the import table)
//! that the viewer renders like any other text content.
//!
//! Imports matching a curated set of injection / execution / anti-analysis APIs
//! are flagged, and high-entropy sections (a packing / encryption tell) are
//! called out — the two cheapest signals for triaging an unknown sample.

use goblin::Object;

mod suspicious;

/// Shannon entropy threshold above which a section is flagged as likely
/// packed / encrypted (max entropy is 8.0 bits per byte).
const HIGH_ENTROPY: f64 = 7.0;

/// Shannon entropy of `data` in bits per byte (0.0 for empty input).
fn entropy(data: &[u8]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut counts = [0u64; 256];
    for &b in data {
        counts[b as usize] += 1;
    }
    let len = data.len() as f64;
    let h: f64 = counts
        .iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / len;
            -p * p.log2()
        })
        .sum();
    // A single-value section yields -0.0 (−1·log2(1)); clamp so it prints as 0.00.
    h.max(0.0)
}

/// Slice `[off, off+len)` of `bytes`, clamped to what's actually present (a
/// section header can claim more than the file contains).
fn slice(bytes: &[u8], off: usize, len: usize) -> &[u8] {
    let end = off.saturating_add(len).min(bytes.len());
    bytes.get(off.min(end)..end).unwrap_or(&[])
}

/// Indent prefix for an import row: a `(!)` flag for suspicious APIs, plain
/// spaces otherwise, kept the same width so the names stay column-aligned.
fn import_prefix(flagged: bool) -> &'static str {
    if flagged { "(!) " } else { "    " }
}

/// Append a section row and flag it when its entropy is high.
fn push_section(out: &mut Vec<String>, name: &str, vaddr: u64, vsize: u64, raw: &[u8]) {
    let e = entropy(raw);
    let flag = if e >= HIGH_ENTROPY {
        "  <- high (packed/encrypted?)"
    } else {
        ""
    };
    out.push(format!(
        "  {name:<14} vaddr 0x{vaddr:<10x} vsize 0x{vsize:<8x} raw {:<9} entropy {e:.2}{flag}",
        raw.len()
    ));
}

/// Build a structure report for `bytes`, or `None` when it isn't a recognized
/// PE / ELF / Mach-O (so the caller can fall back to a hex view).
pub fn report(bytes: &[u8]) -> Option<Vec<String>> {
    match Object::parse(bytes).ok()? {
        Object::PE(pe) => Some(pe_report(&pe, bytes)),
        Object::Elf(elf) => Some(elf_report(&elf, bytes)),
        Object::Mach(goblin::mach::Mach::Binary(macho)) => Some(macho_report(&macho)),
        Object::Mach(goblin::mach::Mach::Fat(fat)) => Some(fat_report(&fat)),
        _ => None,
    }
}

/// Map a PE COFF machine id to a short architecture name.
fn pe_machine(m: u16) -> &'static str {
    match m {
        0x014c => "x86",
        0x8664 => "x86-64",
        0x01c0 | 0x01c4 => "ARM",
        0xaa64 => "ARM64",
        0x0200 => "IA64",
        _ => "unknown",
    }
}

fn pe_report(pe: &goblin::pe::PE, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let coff = &pe.header.coff_header;
    let kind = if pe.is_64 { "PE32+" } else { "PE32" };
    let role = if pe.is_lib { "DLL" } else { "executable" };
    out.push(format!("{kind} {role}  ({})", pe_machine(coff.machine)));
    out.push(format!(
        "entry 0x{:x}  image_base 0x{:x}",
        pe.entry, pe.image_base
    ));
    let ts = coff.time_date_stamp;
    out.push(format!(
        "compiled {}  (timestamp 0x{ts:x})",
        format_unix(ts as i64)
    ));
    if let Some(opt) = pe.header.optional_header {
        out.push(format!(
            "subsystem {}  dll_characteristics 0x{:x}",
            opt.windows_fields.subsystem, opt.windows_fields.dll_characteristics
        ));
    }

    out.push(String::new());
    out.push(format!("sections ({})", pe.sections.len()));
    for s in &pe.sections {
        let name = s.name().unwrap_or("?");
        let raw = slice(
            bytes,
            s.pointer_to_raw_data as usize,
            s.size_of_raw_data as usize,
        );
        push_section(
            &mut out,
            name,
            s.virtual_address as u64,
            s.virtual_size as u64,
            raw,
        );
    }

    // Group imports by DLL and flag suspicious functions.
    out.push(String::new());
    let total = pe.imports.len();
    let mut sus: Vec<String> = Vec::new();
    let mut by_dll: Vec<(String, Vec<(String, bool)>)> = Vec::new();
    for imp in &pe.imports {
        let name = imp.name.to_string();
        let flagged = suspicious::is_suspicious(&name);
        if flagged && !sus.contains(&name) {
            sus.push(name.clone());
        }
        match by_dll.iter_mut().find(|(d, _)| *d == imp.dll) {
            Some((_, fns)) => fns.push((name, flagged)),
            None => by_dll.push((imp.dll.to_string(), vec![(name, flagged)])),
        }
    }
    out.push(format!(
        "imports ({} dlls, {total} funcs){}",
        by_dll.len(),
        suspicious_summary(sus.len())
    ));
    for (dll, fns) in &by_dll {
        out.push(format!("  {dll}"));
        for (name, flagged) in fns {
            out.push(format!("    {}{name}", import_prefix(*flagged)));
        }
    }
    push_suspicious_list(&mut out, &sus);
    out
}

/// Map an ELF machine id (`e_machine`) to a short architecture name.
fn elf_machine(m: u16) -> &'static str {
    use goblin::elf::header;
    match m {
        header::EM_386 => "x86",
        header::EM_X86_64 => "x86-64",
        header::EM_ARM => "ARM",
        header::EM_AARCH64 => "ARM64",
        header::EM_RISCV => "RISC-V",
        header::EM_MIPS => "MIPS",
        _ => "unknown",
    }
}

fn elf_report(elf: &goblin::elf::Elf, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let class = if elf.is_64 { "ELF64" } else { "ELF32" };
    let endian = if elf.little_endian { "LE" } else { "BE" };
    let kind = if elf.is_lib {
        "shared object"
    } else {
        "executable"
    };
    out.push(format!(
        "{class} {kind}  ({}, {endian})",
        elf_machine(elf.header.e_machine)
    ));
    out.push(format!("entry 0x{:x}", elf.entry));
    if let Some(interp) = elf.interpreter {
        out.push(format!("interpreter {interp}"));
    }
    if !elf.libraries.is_empty() {
        out.push(format!("needed: {}", elf.libraries.join(", ")));
    }

    out.push(String::new());
    out.push(format!("sections ({})", elf.section_headers.len()));
    for sh in &elf.section_headers {
        let name = elf.shdr_strtab.get_at(sh.sh_name).unwrap_or("?");
        // SHT_NOBITS (.bss) occupies no file bytes — entropy isn't meaningful.
        let raw = if sh.sh_type == goblin::elf::section_header::SHT_NOBITS {
            &[][..]
        } else {
            slice(bytes, sh.sh_offset as usize, sh.sh_size as usize)
        };
        push_section(&mut out, name, sh.sh_addr, sh.sh_size, raw);
    }

    // Imported (undefined) dynamic symbols.
    out.push(String::new());
    let mut imports: Vec<String> = Vec::new();
    let mut sus: Vec<String> = Vec::new();
    for sym in elf.dynsyms.iter() {
        if !sym.is_import() {
            continue;
        }
        let name = elf.dynstrtab.get_at(sym.st_name).unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        if suspicious::is_suspicious(&name) && !sus.contains(&name) {
            sus.push(name.clone());
        }
        imports.push(name);
    }
    out.push(format!(
        "imports ({}){}",
        imports.len(),
        suspicious_summary(sus.len())
    ));
    for name in &imports {
        let flagged = suspicious::is_suspicious(name);
        out.push(format!("    {}{name}", import_prefix(flagged)));
    }
    push_suspicious_list(&mut out, &sus);
    out
}

fn macho_report(macho: &goblin::mach::MachO) -> Vec<String> {
    let mut out = Vec::new();
    let class = if macho.is_64 {
        "Mach-O 64"
    } else {
        "Mach-O 32"
    };
    out.push(format!("{class}  (cputype 0x{:x})", macho.header.cputype));
    out.push(format!("entry 0x{:x}", macho.entry));
    if !macho.libs.is_empty() {
        out.push(format!("libs: {}", macho.libs.join(", ")));
    }

    out.push(String::new());
    out.push("sections".to_string());
    for seg in &macho.segments {
        for (s, data) in seg.into_iter().flatten() {
            let name = format!("{},{}", s.segname().unwrap_or("?"), s.name().unwrap_or("?"));
            push_section(&mut out, &name, s.addr, s.size, data);
        }
    }

    out.push(String::new());
    let mut sus: Vec<String> = Vec::new();
    let imports = macho.imports().unwrap_or_default();
    out.push(format!(
        "imports ({}){}",
        imports.len(),
        suspicious_summary_pending(&imports, &mut sus)
    ));
    for imp in &imports {
        let flagged = suspicious::is_suspicious(imp.name);
        out.push(format!(
            "    {}{:30} {}",
            import_prefix(flagged),
            imp.name,
            imp.dylib
        ));
    }
    push_suspicious_list(&mut out, &sus);
    out
}

/// Collect suspicious names from Mach-O imports (which borrow from the parser)
/// into `sus`, returning the summary suffix. Kept separate so the report builder
/// doesn't have to clone every import name twice.
fn suspicious_summary_pending(
    imports: &[goblin::mach::imports::Import],
    sus: &mut Vec<String>,
) -> String {
    for imp in imports {
        if suspicious::is_suspicious(imp.name) && !sus.iter().any(|s| s == imp.name) {
            sus.push(imp.name.to_string());
        }
    }
    suspicious_summary(sus.len())
}

fn fat_report(fat: &goblin::mach::MultiArch) -> Vec<String> {
    use goblin::mach::SingleArch;
    let mut out = vec![format!("Fat Mach-O  ({} slices)", fat.narches)];
    if let Ok(arches) = fat.arches() {
        for a in arches {
            out.push(format!(
                "  cputype 0x{:x}  offset 0x{:x}  size 0x{:x}",
                a.cputype, a.offset, a.size
            ));
        }
    }
    // Expand each slice's full report so fat binaries (the norm on macOS) get the
    // same section/import detail as a thin one.
    for (i, entry) in fat.into_iter().enumerate() {
        out.push(String::new());
        out.push(format!("──── slice {i} ────"));
        match entry {
            Ok(SingleArch::MachO(m)) => out.extend(macho_report(&m)),
            Ok(SingleArch::Archive(_)) => out.push("(static archive slice)".into()),
            Err(e) => out.push(format!("(unparsable slice: {e})")),
        }
    }
    out
}

fn suspicious_summary(n: usize) -> String {
    if n > 0 {
        format!("  (!) {n} suspicious")
    } else {
        String::new()
    }
}

fn push_suspicious_list(out: &mut Vec<String>, sus: &[String]) {
    if sus.is_empty() {
        return;
    }
    out.push(String::new());
    out.push(format!("suspicious APIs ({}):", sus.len()));
    out.push(format!("  {}", sus.join(", ")));
}

fn format_unix(secs: i64) -> String {
    use chrono::{TimeZone, Utc};
    match Utc.timestamp_opt(secs, 0).single() {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%SZ").to_string(),
        None => "--".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entropy_extremes() {
        assert_eq!(entropy(&[]), 0.0);
        // All-identical bytes carry no information.
        assert_eq!(entropy(&[0xaa; 1000]), 0.0);
        // A uniform spread over all 256 values is maximal (8 bits/byte).
        let all: Vec<u8> = (0..=255).collect();
        assert!((entropy(&all) - 8.0).abs() < 1e-9);
    }

    #[test]
    fn slice_clamps_past_eof() {
        let b = [1u8, 2, 3, 4];
        assert_eq!(slice(&b, 2, 100), &[3, 4]);
        assert_eq!(slice(&b, 10, 4), &[] as &[u8]);
    }

    #[test]
    fn report_rejects_non_executable() {
        assert!(report(b"just some plain text, not a binary").is_none());
    }
}
