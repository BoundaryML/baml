//! ELF note injection for `baml pack` on Linux.
//!
//! # Why this exists (and isn't just `libsui::Elf::append`)
//!
//! `baml pack` embeds the `PackEnvelope` as an ELF note section
//! (`.note.sui`) so the packed host can read it back at startup via
//! `libsui::find_section` (which walks `PT_NOTE` segments in memory through
//! `dl_iterate_phdr`). libsui 0.14.0's writer, `Elf::append`, has a layout
//! bug that corrupts the embedded data whenever the host binary's last
//! writable `PT_LOAD` segment has a large `.bss`:
//!
//!   - It appends `.note.sui` to that last RW `PT_LOAD` and derives the
//!     section's virtual address purely from its *file* offset
//!     (`sh_addr = seg.p_vaddr + (sh_offset - seg.p_offset)`).
//!   - That ignores `.bss` (`SHT_NOBITS`), whose *memory* image extends past
//!     its (zero-length) *file* image — i.e. the segment's `p_memsz` is
//!     larger than its `p_filesz`.
//!   - When `.bss` is big enough, the note's virtual address lands *inside*
//!     the `.bss` memory range. The program's zero-initialized globals then
//!     alias the embedded envelope: at startup they read the note's bytes as
//!     their initial values (garbage globals → SIGSEGV) and overwrite the
//!     envelope (so `find_section` reads corrupt data → borsh fails with
//!     "invalid utf-8" / "Unexpected variant tag").
//!
//! This reproduces 100% deterministically whenever `baml-pack-host` is built
//! in the same cargo invocation as `baml_cli` (`cargo build -p baml_cli -p
//! baml_pack_host`): feature unification across the shared dependency tree
//! pulls more static state into the host, inflating `.bss` past the overlap
//! threshold. Built alone, the host's `.bss` is small enough to fit under the
//! page boundary and the bug stays hidden — which is how it shipped (the
//! team develops on macOS, where libsui takes the Mach-O path).
//!
//! # The fix
//!
//! We can't grow the program header table to give the note its own fresh
//! `PT_LOAD` — release hosts pack the phdr table flush against `.interp`,
//! leaving zero slack, and `object`'s ELF builder refuses to relocate alloc
//! sections to make room. So we keep libsui's strategy (reuse the existing
//! `PT_NOTE` phdr, append `.note.sui` to the last RW `PT_LOAD`) and change
//! exactly one thing: place the note past the segment's whole *memory* image
//! (`p_offset + p_memsz`), not just past its file bytes, so its virtual
//! address always clears `.bss`. The note's on-wire format is byte-for-byte
//! identical to libsui's, so the unmodified `libsui::find_section` reader
//! still parses it.
//!
//! The writer logic mirrors `libsui::Elf::append` (MIT-licensed, Divy
//! Srivastava / the Deno authors); the placement math is the only deviation.

use std::io::Write;

use anyhow::{Result, anyhow};
use object::{
    build::elf as e,
    elf::{PF_R, PT_NOTE, SHF_ALLOC, SHT_NOBITS, SHT_NOTE},
    endian::Endianness,
    read::elf::{FileHeader, ProgramHeader},
};

/// Note name (NUL-terminated) and vendor type tag, matching libsui's reader
/// so `libsui::find_section` parses what we write.
const ELF_NOTE_NAME: &[u8] = b"SUI\0";
const ELF_NOTE_TYPE_SECTION_DATA: u32 = 0x5355_4901;

fn align_up(value: u64, align: u64) -> u64 {
    if align <= 1 {
        value
    } else {
        (value + (align - 1)) & !(align - 1)
    }
}

/// Build the ELF note payload (header + name + descriptor). The descriptor
/// is `u16 name_len | section_name | section_data`, identical to
/// `libsui::build_elf_note_payload`, so the host's `libsui::find_section` →
/// `parse_elf_note_desc` reads it back unchanged.
fn build_elf_note_payload(section_name: &str, section_data: &[u8]) -> Vec<u8> {
    let name_len = u16::try_from(section_name.len()).expect("section name too long");

    let mut desc = Vec::with_capacity(2 + section_name.len() + section_data.len());
    desc.extend_from_slice(&name_len.to_le_bytes());
    desc.extend_from_slice(section_name.as_bytes());
    desc.extend_from_slice(section_data);

    let mut note = Vec::new();
    note.extend_from_slice(&(ELF_NOTE_NAME.len() as u32).to_le_bytes());
    note.extend_from_slice(&(desc.len() as u32).to_le_bytes());
    note.extend_from_slice(&ELF_NOTE_TYPE_SECTION_DATA.to_le_bytes());
    note.extend_from_slice(ELF_NOTE_NAME);
    note.resize(align_up(note.len() as u64, 4) as usize, 0);
    note.extend_from_slice(&desc);
    note.resize(align_up(note.len() as u64, 4) as usize, 0);
    note
}

/// Append `data` to `host` (a 64-bit ELF) as a `.note.sui` section and write
/// the result to `writer`. Drop-in replacement for `libsui::Elf::append`
/// with the `.bss`-overlap fix documented at the module level.
pub fn append_note<W: Write>(
    host: &[u8],
    section_name: &str,
    data: &[u8],
    writer: &mut W,
) -> Result<()> {
    let note_data = build_elf_note_payload(section_name, data);

    // Existing `PT_NOTE` section headers are not preserved by the builder
    // once we repurpose the note segment; copy their contents into
    // `.note.sui` so the original notes (e.g. `.note.gnu.build-id`) survive.
    let combined_note_data = {
        let existing = object::read::elf::ElfFile64::<Endianness, _>::parse(host)
            .ok()
            .and_then(|elf_file| {
                let endian = elf_file.endian();
                let header = elf_file.elf_header();
                let segments = header.program_headers(endian, host).ok()?;
                for segment in segments {
                    if segment.p_type(endian) != PT_NOTE {
                        continue;
                    }
                    let data = segment.data(endian, host).ok()?;
                    if !data.is_empty() {
                        return Some(data);
                    }
                }
                None
            });
        if let Some(existing) = existing {
            let mut combined = Vec::with_capacity(existing.len() + note_data.len());
            combined.extend_from_slice(existing);
            combined.extend_from_slice(&note_data);
            combined
        } else {
            note_data
        }
    };

    let mut builder =
        e::Builder::read(host).map_err(|err| anyhow!("Failed to parse host ELF: {err}"))?;

    let section = builder.sections.add();
    section.name = ".note.sui".into();
    section.sh_type = SHT_NOTE;
    section.sh_flags = u64::from(SHF_ALLOC);
    section.sh_addralign = 4;
    section.data = e::SectionData::Note(combined_note_data.into());
    let section_id = section.id();

    builder.set_section_sizes();

    // Highest file offset any non-NOBITS section reaches.
    let mut max_end = 0u64;
    for existing in builder.sections.iter() {
        if existing.delete {
            continue;
        }
        let filesz = if existing.sh_type == SHT_NOBITS {
            0
        } else {
            existing.sh_size
        };
        max_end = max_end.max(existing.sh_offset + filesz);
    }

    // The note rides along in the last (by file extent) PT_LOAD segment.
    let load_segment_id = builder
        .segments
        .iter()
        .filter(|segment| segment.is_load())
        .max_by_key(|segment| segment.p_offset + segment.p_filesz)
        .map(|segment| segment.id());

    {
        let section = builder.sections.get_mut(section_id);
        let (align, min_offset) = if let Some(load_segment_id) = load_segment_id {
            let seg = builder.segments.get(load_segment_id);
            let align = seg.p_align.max(section.sh_addralign).max(1);
            // *** The fix. ***
            // The carrier segment's memory image (`p_memsz`) can exceed its
            // file image (`p_filesz`) because of a trailing `.bss`
            // (`SHT_NOBITS`). Placing the note right after the file bytes
            // (libsui's behavior) gives it a virtual address inside that
            // NOBITS tail, so the program's zero-init globals alias — and
            // clobber — the embedded envelope. Push the note past the
            // segment's full memory image so its mapped range clears `.bss`.
            (align, seg.p_offset + seg.p_memsz)
        } else {
            (section.sh_addralign.max(1), 0)
        };
        section.sh_offset = align_up(max_end.max(min_offset), align);
        section.sh_addr = if let Some(load_segment_id) = load_segment_id {
            let seg = builder.segments.get(load_segment_id);
            seg.p_vaddr + (section.sh_offset - seg.p_offset)
        } else {
            0
        };
    }

    // Extend the carrier segment to cover the note in both file and memory.
    if let Some(load_segment_id) = load_segment_id {
        let section = builder.sections.get_mut(section_id);
        let section_end = section.sh_offset + section.sh_size;
        let mem_end = section.sh_addr + section.sh_size;
        let seg = builder.segments.get_mut(load_segment_id);
        if section_end > seg.p_offset + seg.p_filesz {
            seg.p_filesz = section_end - seg.p_offset;
        }
        if mem_end > seg.p_vaddr + seg.p_memsz {
            seg.p_memsz = mem_end - seg.p_vaddr;
        }
        if !seg.sections.contains(&section_id) {
            seg.sections.push(section_id);
        }
    }

    // Repoint the existing PT_NOTE phdr at `.note.sui` (or add one if the
    // host had none — rare, but matches libsui). Reusing the slot avoids
    // growing the program header table, which release hosts pack flush
    // against `.interp` with no room to spare.
    let note_segment_id = builder
        .segments
        .iter()
        .find(|segment| segment.p_type == PT_NOTE)
        .map(|segment| segment.id());

    let section = builder.sections.get_mut(section_id);
    let (sh_offset, sh_addr, sh_size) = (section.sh_offset, section.sh_addr, section.sh_size);
    let segment = match note_segment_id {
        Some(id) => {
            let segment = builder.segments.get_mut(id);
            segment.sections.clear();
            segment.sections.push(section_id);
            segment
        }
        None => {
            let segment = builder.segments.add();
            segment.sections.push(section_id);
            segment
        }
    };
    segment.p_type = PT_NOTE;
    segment.p_flags = PF_R;
    segment.p_align = 4;
    segment.p_offset = sh_offset;
    segment.p_vaddr = sh_addr;
    segment.p_paddr = sh_addr;
    segment.p_filesz = sh_size;
    segment.p_memsz = sh_size;

    let mut out = Vec::new();
    builder
        .write(&mut out)
        .map_err(|err| anyhow!("Failed to write packed ELF: {err}"))?;
    writer.write_all(&out)?;
    Ok(())
}
