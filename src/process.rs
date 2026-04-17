//! Top-level source orchestration — the `run()` dispatcher that decides
//! whether a given source should be rendered or streamed through as `cat`,
//! plus the pure passthrough pump that honors the cat-compatibility contract
//! when stdout is not a TTY.

use std::fs::File;
use std::io::{self, BufReader, Read, Write};

use crate::config::Source;
use crate::markdown::render;
use crate::terminal::TermConfig;
use crate::utils::source_base_dir;

pub fn run(source: &Source, term: &TermConfig) -> io::Result<()> {
    if !term.render_active {
        return passthrough(source);
    }

    let mut buf = String::new();
    match source {
        Source::Stdin => {
            io::stdin().read_to_string(&mut buf)?;
        }
        Source::File(p) => {
            let mut f = BufReader::new(File::open(p)?);
            f.read_to_string(&mut buf)?;
        }
    }

    // Local-image path resolution is rooted at the source file's parent
    // directory (or refused entirely for stdin, since there is no safe
    // root). `source_base_dir()` centralizes that derivation so the policy
    // stays in one place.
    let base = match source {
        Source::File(_) => Some(source_base_dir(source)),
        Source::Stdin => None,
    };
    render(&buf, term, base)
}

pub fn passthrough(source: &Source) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    match source {
        Source::Stdin => {
            let stdin = io::stdin();
            let mut inp = stdin.lock();
            passthrough_bytes(&mut inp, &mut out)?;
        }
        Source::File(p) => {
            let mut f = File::open(p)?;
            passthrough_bytes(&mut f, &mut out)?;
        }
    }
    Ok(())
}

/// Pure passthrough byte pump. Extracted as a seam so cat-parity tests can
/// feed arbitrary readers/writers without needing a real TTY.
pub fn passthrough_bytes<R: Read, W: Write>(input: &mut R, output: &mut W) -> io::Result<u64> {
    io::copy(input, output)
}

/// Pure predicate encoding the cat-compatibility contract: render only when
/// stdout is a real TTY, or the user opts in with --force-color / FORCE_COLOR.
pub fn should_render(is_tty: bool, force_color: bool) -> bool {
    is_tty || force_color
}
