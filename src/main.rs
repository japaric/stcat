#![deny(warnings)]

#[macro_use]
extern crate clap;
#[macro_use]
extern crate error_chain;
extern crate xmas_elf;

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Write};

use clap::{App, Arg};
use xmas_elf::ElfFile;
use xmas_elf::sections::SectionData;
use xmas_elf::symbol_table::Entry;

use errors::*;

mod errors {
    error_chain!();
}

quick_main!(run);

// Log level
#[derive(Eq, Ord, PartialEq, PartialOrd)]
enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Level::Trace => f.write_str("TRACE"),
            Level::Debug => f.write_str("DEBUG"),
            Level::Info => f.write_str("INFO"),
            Level::Warn => f.write_str("WARN"),
            Level::Error => f.write_str("ERROR"),
        }
    }
}

struct Message<'a> {
    level: Level,
    string: &'a str,
}

fn run() -> Result<()> {
    let args = App::new("stcat")
        .author(crate_authors!())
        .version(crate_version!())
        .about("Decodes strings logged via the `stlog` framework")
        .arg(
            Arg::with_name("elf")
                .help("ELF file where log strings are stored")
                .short("e")
                .takes_value(true)
                .value_name("ELF"),
        )
        .arg(
            Arg::with_name("debug")
                .help("increases the debug level (default = info)")
                .short("d")
                .multiple(true)
                .takes_value(false),
        )
        .get_matches();

    let path = args.value_of("elf").unwrap();
    let requested_level = match args.occurrences_of("debug") {
        0 => Level::Info,
        1 => Level::Debug,
        _ => Level::Trace,
    };

    let mut file = File::open(path).chain_err(
        || format!("couldn't open {}", path),
    )?;
    let mut contents = vec![];
    file.read_to_end(&mut contents).chain_err(|| {
            format!("couldn't read {}", path)
        })?;

    let elf = ElfFile::new(&contents);

    let table = if let Some(sh) = elf.find_section_by_name(".symtab") {
        let data = sh.get_data(&elf)?;

        if let SectionData::SymbolTable32(entries) = data {
            let (strace, shndx) = entries
                .iter()
                .find(|entry| entry.get_name(&elf) == Ok("_sstlog_trace"))
                .map(|entry| (entry.value(), entry.shndx()))
                .ok_or("_sstlog_trace symbol not found")?;

            let mut etrace = None;
            let mut sdebug = None;
            let mut edebug = None;
            let mut sinfo = None;
            let mut einfo = None;
            let mut swarn = None;
            let mut ewarn = None;
            let mut serror = None;
            let mut eerror = None;

            // unclassified messages
            let mut messages = vec![];

            for entry in entries {
                if entry.shndx() == shndx {
                    // magic `info` value
                    const MAGIC: u8 = 1;

                    if entry.info() == MAGIC {
                        messages.push(entry);
                        continue;
                    }

                    if entry.get_name(&elf) == Ok("_estlog_trace") {
                        etrace = Some(entry.value());
                    } else if entry.get_name(&elf) == Ok("_sstlog_debug") {
                        sdebug = Some(entry.value());
                    } else if entry.get_name(&elf) == Ok("_estlog_debug") {
                        edebug = Some(entry.value());
                    } else if entry.get_name(&elf) == Ok("_sstlog_info") {
                        sinfo = Some(entry.value());
                    } else if entry.get_name(&elf) == Ok("_estlog_info") {
                        einfo = Some(entry.value());
                    } else if entry.get_name(&elf) == Ok("_sstlog_warn") {
                        swarn = Some(entry.value());
                    } else if entry.get_name(&elf) == Ok("_estlog_warn") {
                        ewarn = Some(entry.value());
                    } else if entry.get_name(&elf) == Ok("_sstlog_error") {
                        serror = Some(entry.value());
                    } else if entry.get_name(&elf) == Ok("_estlog_error") {
                        eerror = Some(entry.value());
                    }
                }
            }

            let etrace = etrace.ok_or("_estlog_trace symbol not found")?;
            let sdebug = sdebug.ok_or("_sstlog_debug symbol not found")?;
            let edebug = edebug.ok_or("_estlog_debug symbol not found")?;
            let sinfo = sinfo.ok_or("_sstlog_info symbol not found")?;
            let einfo = einfo.ok_or("_estlog_info symbol not found")?;
            let swarn = swarn.ok_or("_sstlog_warn symbol not found")?;
            let ewarn = ewarn.ok_or("_estlog_warn symbol not found")?;
            let serror = serror.ok_or("_sstlog_error symbol not found")?;
            let eerror = eerror.ok_or("_estlog_error symbol not found")?;

            // id -> message
            let mut table = HashMap::new();

            for entry in messages {
                let value = entry.value();

                let level = if value >= strace && value < etrace {
                    Level::Trace
                } else if value >= sdebug && value < edebug {
                    Level::Debug
                } else if value >= sinfo && value < einfo {
                    Level::Info
                } else if value >= swarn && value < ewarn {
                    Level::Warn
                } else if value >= serror && value < eerror {
                    Level::Error
                } else {
                    unreachable!()
                };

                table.insert(
                    value,
                    Message {
                        level: level,
                        string: entry.get_name(&elf)?,
                    },
                );
            }

            table
        } else {
            unreachable!()
        }
    } else {
        bail!("{} has no .symtab section", path);
    };

    let stderr = io::stderr();
    let stdin = io::stdin();
    let stdout = io::stdout();

    let stdin = stdin.lock();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();

    for byte in stdin.bytes() {
        let byte = byte.chain_err(|| "I/O error")? as u64;

        if let Some(message) = table.get(&byte) {
            if message.level >= requested_level {
                writeln!(stdout, "{} {}", message.level, message.string)
                    .chain_err(|| "I/O error")?;
            }
        } else {
            writeln!(stderr, "unknown message id {}", byte).chain_err(
                || "I/O error",
            )?;
        }
    }

    Ok(())
}
