//! `mat` — binary entry.
//!
//! The entire pipeline (arg parsing → terminal detection → per-source render
//! or passthrough) lives in the `mat` library crate under `src/`. This file
//! only wires `fn main()` to the library and maps its results onto
//! `ExitCode`.

use std::env;
use std::process::ExitCode;

use mat::config::{ExitAction, parse_args, print_usage, print_version};
use mat::process::run;
use mat::terminal::resolve_terminal;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let cfg = match parse_args(&args) {
        Ok(c) => c,
        Err(ExitAction::PrintUsage) => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        Err(ExitAction::PrintVersion) => {
            print_version();
            return ExitCode::SUCCESS;
        }
        Err(ExitAction::Usage(msg)) => {
            eprintln!("mat: {msg}");
            eprintln!("Try 'mat --help' for more information.");
            return ExitCode::from(2);
        }
    };

    let term = resolve_terminal(&cfg);

    let mut any_err = false;
    for source in &cfg.sources {
        if let Err(e) = run(source, &term) {
            eprintln!("mat: {}: {}", source.display(), e);
            any_err = true;
        }
    }

    if any_err {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
