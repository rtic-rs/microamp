//! Build a μAMP application

#![deny(rust_2018_compatibility)]
#![deny(rust_2018_idioms)]
#![deny(warnings)]

use core::{fmt, iter::FromIterator};
use std::{
    collections::BTreeMap,
    env, fs,
    process::{self, Command},
    time::SystemTime,
};

use cargo_project::{Artifact, Profile, Project};
use clap::{App, Arg};
use exitfailure::ExitFailure;
use failure::{bail, ensure, format_err};
use filetime::FileTime;
use tempdir::TempDir;
use walkdir::WalkDir;
use xmas_elf::{sections::SectionData, symbol_table::Entry, ElfFile};

fn main() -> Result<(), ExitFailure> {
    process::exit(run()?)
}

fn run() -> Result<i32, failure::Error> {
    let matches = App::new("cargo-microamp")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Jorge Aparicio <jorge@japaric.io>")
        .about("Build a μAMP application")
        // as this is used as a Cargo subcommand the first argument will be the name of the binary
        // we ignore this argument
        .arg(Arg::with_name("binary-name").hidden(true))
        .arg(
            Arg::with_name("cores")
                .long("cores")
                .short("c")
                .takes_value(true)
                .value_name("N")
                .help("Number of cores to build this program for (default: 2)"),
        )
        // TODO we need one target per core
        // .arg(
        //     Arg::with_name("target")
        //         .long("target")
        //         .takes_value(true)
        //         .value_name("TRIPLE")
        //         .help("Target triple for which the code is compiled"),
        // )
        .arg(
            Arg::with_name("verbose")
                .long("verbose")
                .short("v")
                .help("Use verbose output"),
        )
        .arg(
            Arg::with_name("example")
                .long("example")
                .takes_value(true)
                .value_name("NAME")
                .help("Build only the specified example"),
        )
        .arg(
            Arg::with_name("bin")
                .long("bin")
                .takes_value(true)
                .value_name("BIN")
                .help("Build only the specified binary"),
        )
        .arg(
            Arg::with_name("features")
                .long("features")
                .takes_value(true)
                .value_name("FEATURES")
                .help("Space-separated list of features to activate"),
        )
        .arg(
            Arg::with_name("all-features")
                .long("all-features")
                .takes_value(false)
                .help("Activate all available features"),
        )
        .arg(
            Arg::with_name("release")
                .long("release")
                .help("Build artifacts in release mode, with optimizations"),
        )
        .arg(
            Arg::with_name("check")
                .long("check")
                .help("Do not link; only compile check"),
        )
        .get_matches();

    let cores = matches
        .value_of("cores")
        .map(str::parse)
        .unwrap_or(Ok(2_usize))?;
    let check = matches.is_present("check");
    let build_profile = if matches.is_present("release") {
        if check {
            bail!("can't specify both `--check` and `--release`");
        }

        Profile::Release
    } else {
        Profile::Dev
    };
    let verbose = matches.is_present("verbose");

    let artifact = match (matches.value_of("bin"), matches.value_of("example")) {
        (Some(bin), None) => Artifact::Bin(bin),
        (None, Some(ex)) => Artifact::Example(ex),
        _ => bail!("please specify --example <NAME> or --bin <NAME>"),
    };

    let meta = rustc_version::version_meta()?;
    let host = meta.host;
    let project = Project::query(env::current_dir()?)?;

    // "touch" some source file to trigger a rebuild
    let root = project.toml().parent().expect("UNREACHABLE");
    let now = FileTime::from_system_time(SystemTime::now());
    match artifact {
        Artifact::Bin(bin) => {
            if bin == project.name() {
                filetime::set_file_times(root.join("src/main.rs"), now, now)?
            } else {
                filetime::set_file_times(root.join(format!("src/bin/{}.rs", bin)), now, now)?
            }
        }

        Artifact::Example(ex) => {
            filetime::set_file_times(root.join(format!("examples/{}.rs", ex)), now, now)?
        }

        Artifact::Lib => filetime::set_file_times(root.join("src/lib.rs"), now, now)?,

        _ => {
            // look for some rust source file and "touch" it
            let src = root.join("src");
            let haystack = if src.exists() { &src } else { root };

            for entry in WalkDir::new(haystack) {
                let entry = entry?;
                let path = entry.path();

                if path.extension().map(|ext| ext == "rs").unwrap_or(false) {
                    filetime::set_file_times(path, now, now)?;
                    break;
                }
            }
        }
    }

    let cargo = || {
        let mut c = Command::new("cargo");
        c.arg("rustc");
        match artifact {
            Artifact::Bin(bin) => {
                c.args(&["--bin", bin]);
            }
            Artifact::Example(ex) => {
                c.args(&["--example", ex]);
            }
            _ => {}
        }
        if build_profile.is_release() {
            c.arg("--release");
        }
        if matches.is_present("all-features") {
            c.arg("--all-features");
        } else if let Some(features) = matches.value_of("features") {
            c.args(&["--features", features]);
        }
        c.arg("--");
        c
    };

    if check {
        let mut c = cargo();
        c.args(&[
            "--cfg",
            "microamp",
            "-C",
            "linker=microamp-true",
            "-A",
            "warnings",
        ]);
        if verbose {
            eprintln!("{:?}", c);
        }
        let status = c.status()?;
        if !status.success() {
            return Ok(status.code().unwrap_or(1));
        }

        for i in 0..cores {
            let mut c = cargo();
            c.arg("--cfg");
            c.arg(&format!("core=\"{}\"", i));
            c.args(&["-C", "linker=microamp-true"]);
            if verbose {
                eprintln!("{:?}", c);
            }
            let status = c.status()?;
            if !status.success() {
                return Ok(status.code().unwrap_or(1));
            }
        }
    } else {
        let mut c = cargo();
        c.args(&[
            "-C",
            "lto",
            "--cfg",
            "microamp",
            "--emit=obj",
            "-A",
            "warnings",
            "-C",
            "linker=microamp-true",
        ]);
        if verbose {
            eprintln!("{:?}", c);
        }
        let status = c.status()?;
        if !status.success() {
            return Ok(status.code().unwrap_or(1));
        }

        let path = project.path(artifact, build_profile, None, &host)?;
        let parent = path.parent().expect("unreachable");
        let (haystack, name) = match artifact {
            Artifact::Bin(bin) => (parent.join("deps"), bin),
            Artifact::Example(ex) => (parent.to_owned(), ex),
            _ => unreachable!(),
        };

        let prefix = format!("{}-", name.replace('-', "_"));
        let mut so = None;
        // most recently modified
        let mut mrm = SystemTime::UNIX_EPOCH;
        for e in fs::read_dir(haystack)? {
            let e = e?;
            let p = e.path();

            if p.extension().map(|ext| ext == "o").unwrap_or(false)
                && p.file_stem()
                    .expect("unreachable")
                    .to_str()
                    .expect("unreachable")
                    .starts_with(&prefix)
            {
                let modified = e.metadata()?.modified()?;
                if so.is_none() {
                    so = Some(p);
                    mrm = modified;
                } else {
                    if modified > mrm {
                        so = Some(p);
                        mrm = modified;
                    }
                }
            }
        }

        // strip '.text' sections from the shared object file
        let so = so.expect("UNREACHABLE");
        let td = TempDir::new("cargo-microamp")?;
        let obj = td.path().join("microamp-data.o");
        fs::copy(&so, &obj)?;

        // FIXME use a Rust library instead of shelling out to `strip`
        let mut c = Command::new("arm-none-eabi-strip");
        c.args(&["-R", "*", "-R", "!.shared", "--strip-unneeded"])
            .arg(&obj);
        if verbose {
            eprintln!("{:?}", c);
        }

        let status = c.status()?;
        if !status.success() {
            return Ok(status.code().unwrap_or(1));
        }

        // address -> (size, name)
        let mut base: Option<(String, Symbols)> = None;
        for i in 0..cores {
            let mut c = cargo();
            c.args(&[
                "--cfg",
                &format!("core=\"{}\"", i),
                "-C",
                &format!("link-arg=-Tcore{}.x", i),
                "-C",
                &format!("link-arg={}", obj.display()),
            ]);
            if verbose {
                eprintln!("{:?}", c);
            }
            let status = c.status()?;

            if !status.success() {
                return Ok(status.code().unwrap_or(1));
            }

            let filename = format!(
                "{}-{}",
                path.file_name()
                    .expect("unreachable")
                    .to_str()
                    .expect("unreachable"),
                i
            );
            let dst = parent.join(&filename);

            fs::rename(&path, &dst)?;

            let contents = fs::read(&dst)?;
            let elf = ElfFile::new(&contents).map_err(failure::err_msg)?;

            let mut shndx = None;
            for i in 1..elf.header.pt2.sh_count() {
                if let Ok(sh) = elf.section_header(i) {
                    if sh.get_name(&elf) == Ok(".shared") {
                        shndx = Some(i);
                        break;
                    }
                }
            }

            let shndx =
                shndx.ok_or_else(|| format_err!("({}) `.shared` section is missing", filename))?;

            if let Some(symtab) = elf.find_section_by_name(".symtab") {
                match symtab.get_data(&elf).map_err(failure::err_msg)? {
                    SectionData::SymbolTable32(entries) => {
                        if let Some((base_filename, base_symbols)) = &base {
                            let symbols = entries
                                .iter()
                                .filter_map(|entry| {
                                    let size = entry.size();
                                    if entry.shndx() == shndx && size != 0 {
                                        Some((
                                            entry.value(),
                                            (size, entry.get_name(&elf).ok().map(String::from)),
                                        ))
                                    } else {
                                        None
                                    }
                                })
                                .collect::<Symbols>();

                            ensure!(
                                &symbols == base_symbols,
                                "the layout of the `.shared` section doesn't match\n\
                                 {}:\n{:#?}\n{}\n{:#?}",
                                base_filename,
                                base_symbols,
                                filename,
                                symbols,
                            );
                        } else {
                            base = Some((
                                filename,
                                entries
                                    .iter()
                                    .filter_map(|entry| {
                                        let size = entry.size();
                                        if entry.shndx() == shndx && size != 0 {
                                            Some((
                                                entry.value(),
                                                (size, entry.get_name(&elf).ok().map(String::from)),
                                            ))
                                        } else {
                                            None
                                        }
                                    })
                                    .collect(),
                            ))
                        }
                    }

                    SectionData::SymbolTable64(_) => {
                        bail!("64-bit ELF files are not currently supported")
                    }

                    _ => bail!("malformed .symtab section"),
                }
            }
        }
    }

    Ok(0)
}

#[derive(PartialEq)]
struct Symbols {
    entries: BTreeMap<u64, Symbol>,
}

impl FromIterator<(u64, (u64, Option<String>))> for Symbols {
    fn from_iter<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (u64, (u64, Option<String>))>,
    {
        Symbols {
            entries: iter
                .into_iter()
                .map(|(address, (size, name))| (address, Symbol { size, name }))
                .collect(),
        }
    }
}

impl fmt::Debug for Symbols {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct Hex(u64);

        impl fmt::Debug for Hex {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{:#08x}", self.0)
            }
        }

        let mut s = f.debug_map();
        for (address, symbol) in &self.entries {
            s.entry(&Hex(*address), symbol);
        }

        s.finish()
    }
}

#[derive(PartialEq)]
struct Symbol {
    size: u64,
    name: Option<String>,
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("Symbol");
        s.field("size", &self.size);
        if let Some(name) = &self.name {
            s.field("name", name);
        } else {
            s.field("name", &"?");
        }
        s.finish()
    }
}
