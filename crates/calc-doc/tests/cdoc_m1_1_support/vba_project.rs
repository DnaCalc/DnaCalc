//! Change source streams in an existing test project. This creates synthetic
//! source-ingestion controls, never a new Excel observation or a captured answer.
use super::support;
use std::{
    io::{Cursor, Read, Write},
    sync::Arc,
};

pub fn replace_sources(bytes: &[u8], sources: &[(&str, &str)]) -> Vec<u8> {
    let source = Arc::new(calc_doc::MemorySource::new(bytes.to_vec()));
    let document = calc_doc::open(source, &Default::default()).unwrap();
    let loaded = calc_doc::load(&document, &mut Default::default()).unwrap();
    let package = document.package();
    let project = loaded
        .read_vba_project(&Default::default())
        .unwrap()
        .unwrap();
    let mut compound =
        cfb::CompoundFile::open(Cursor::new(package.read_part(&project.part).unwrap())).unwrap();
    for (name, text) in sources {
        let module = project
            .modules
            .iter()
            .find(|module| module.name == *name)
            .unwrap();
        let path = format!("/VBA/{}", module.stream_name);
        let mut bytes = Vec::new();
        compound
            .open_stream(&path)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        bytes.truncate(module.source_offset);
        // MS-OVBA literal-only compressed chunks. No copied caches or p-code is
        // executed by the source frontend; the original directory/offset stays valid.
        bytes.push(1);
        for chunk in text.as_bytes().chunks(3600) {
            let mut literals = Vec::new();
            for group in chunk.chunks(8) {
                literals.push(0);
                literals.extend_from_slice(group);
            }
            bytes.extend_from_slice(&(0xb000u16 | (literals.len() as u16 - 1)).to_le_bytes());
            bytes.extend_from_slice(&literals);
        }
        compound
            .create_stream(&path)
            .unwrap()
            .write_all(&bytes)
            .unwrap();
    }
    let binary = compound.into_inner().into_inner();
    let entries = support::parse_zip(bytes)
        .entries
        .iter()
        .map(|entry| {
            let content = if entry.name == project.part {
                binary.clone()
            } else {
                match entry.method {
                    0 => entry.payload(bytes).to_vec(),
                    8 => miniz_oxide::inflate::decompress_to_vec(entry.payload(bytes)).unwrap(),
                    _ => panic!("unsupported test package compression"),
                }
            };
            support::RawEntry::stored(&entry.name, &content)
        })
        .collect::<Vec<_>>();
    support::build_zip(&entries)
}
