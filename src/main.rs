#![deny(warnings)]

#[macro_use]
extern crate clap;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;
extern crate xmas_elf;

use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read, Write},
    process,
};

use clap::{App, Arg};
use slog::{Drain, Level, Logger};
use slog_async::Async;
use slog_term::{CompactFormat, TermDecorator};
use xmas_elf::{sections::SectionData, symbol_table::{Entry, Type}, ElfFile};

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        process::exit(101);
    }
}

fn run() -> Result<(), failure::Error> {
    let matches = App::new("stcat")
        .about("Decode logs produced by the `stlog` framework")
        .author(crate_authors!())
        .version(crate_version!())
        .arg(
            Arg::with_name("elf")
                .short("e")
                .long("elf")
                .value_name("ELF")
                .takes_value(true)
                .required(true)
                .help("ELF file whose symbol table will be used to decode the logs"),
        ).arg(
            Arg::with_name("filter")
                .short("f")
                .long("filter")
                .value_name("LEVEL")
                .takes_value(true)
                .required(false)
                .help("Decodes only messages of this severity or higher (default: trace)"),
        ).arg(
            Arg::with_name("LOGFILE")
                .required(false)
                .index(1)
                .help("Log file to decode; if omitted stdin will be decoded"),
        ).get_matches();

    let severity = match matches.value_of("severity") {
        Some("error") => Level::Error,
        Some("warning") => Level::Warning,
        Some("info") => Level::Info,
        Some("debug") => Level::Debug,
        Some("trace") => Level::Trace,
        Some(_) => bail!("Level must be one of: error, warning, info, debug or trace"),
        None => Level::Trace,
    };

    let mut bytes = vec![];
    File::open(matches.value_of("elf").unwrap())?.read_to_end(&mut bytes)?;
    let elf = ElfFile::new(&bytes).map_err(failure::err_msg)?;

    let messages = if let Some(symtab) = elf.find_section_by_name(".symtab") {
        match symtab.get_data(&elf).map_err(failure::err_msg)? {
            SectionData::SymbolTable32(entries) => process_symtab(entries, &elf)?,
            SectionData::SymbolTable64(entries) => process_symtab(entries, &elf)?,
            _ => bail!("malformed .symtab section"),
        }
    } else {
        bail!(".symtab section not found");
    };

    let format = CompactFormat::new(TermDecorator::new().stdout().build());
    let stdin = io::stdin();
    let (input, format): (Box<Read>, _) = if let Some(logfile) = matches.value_of("LOGFILE") {
        (
            Box::new(File::open(logfile)?),
            format.use_custom_timestamp(no_timestamp),
        )
    } else {
        (Box::new(stdin.lock()), format.use_local_timestamp())
    };

    let drain = format.build().filter_level(severity).fuse();
    let logger = Logger::root(Async::new(drain).build().fuse(), o!());

    for byte in input.bytes() {
        let address = u64::from(byte?);

        if let Some(message) = messages.get(&address) {
            match message.severity {
                Level::Error => error!(logger, "{}", message.content),
                Level::Warning => warn!(logger, "{}", message.content),
                Level::Info => info!(logger, "{}", message.content),
                Level::Debug => debug!(logger, "{}", message.content),
                Level::Trace => trace!(logger, "{}", message.content),
                _ => {} // unreachable
            }
        }
    }

    Ok(())
}

fn no_timestamp(_: &mut Write) -> io::Result<()> {
    Ok(())
}

fn process_symtab<'a, E>(
    entries: &'a [E],
    elf: &'a ElfFile,
) -> Result<HashMap<u64, Message<'a>>, failure::Error>
where
    E: Entry,
{
    let (warning_start, shndx) = entries
        .iter()
        .find(|entry| entry.get_name(&elf) == Ok("__stlog_warning_start__"))
        .map(|entry| (entry.value(), entry.shndx()))
        .ok_or_else(|| failure::err_msg("symbol `__stlog_warning_start__` not found"))?;

    let mut info_start = None;
    let mut debug_start = None;
    let mut trace_start = None;

    let mut unclassified_messages = vec![];

    for entry in entries {
        if entry.shndx() == shndx {
            if entry.get_type().map_err(failure::err_msg)? == Type::Object {
                unclassified_messages.push(entry);
                continue;
            }

            if entry.get_name(&elf) == Ok("__stlog_info_start__") {
                info_start = Some(entry.value());
            } else if entry.get_name(&elf) == Ok("__stlog_debug_start__") {
                debug_start = Some(entry.value());
            } else if entry.get_name(&elf) == Ok("__stlog_trace_start__") {
                trace_start = Some(entry.value());
            }
        }
    }

    let info_start =
        info_start.ok_or_else(|| failure::err_msg("__stlog_info_start__ symbol not found"))?;
    let debug_start =
        debug_start.ok_or_else(|| failure::err_msg("__stlog_debug_start__ symbol not found"))?;
    let trace_start =
        trace_start.ok_or_else(|| failure::err_msg("__stlog_trace_start__ symbol not found"))?;

    // address -> message
    let mut messages = HashMap::new();

    for entry in unclassified_messages {
        let address = entry.value();

        let severity = if address < warning_start {
            Level::Error
        } else if address >= warning_start && address < info_start {
            Level::Warning
        } else if address >= info_start && address < debug_start {
            Level::Info
        } else if address >= debug_start && address < trace_start {
            Level::Debug
        } else if address >= trace_start {
            Level::Trace
        } else {
            bail!("Found message with invalid address: {}", address)
        };

        messages.insert(
            address,
            Message {
                severity,
                content: entry.get_name(&elf).map_err(failure::err_msg)?,
            },
        );
    }

    Ok(messages)
}

struct Message<'a> {
    severity: Level,
    content: &'a str,
}
