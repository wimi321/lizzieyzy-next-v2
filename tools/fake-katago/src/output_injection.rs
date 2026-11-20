//! Raw output fixtures for the local transport, never linked into the application.
use std::io::{self, Write};

use crate::{RunError, cli::Options};

pub(crate) const PADDED_WARNING: &[u8] = b"{\"warning\":\"fake padded stdout\"}";

// GBK bytes for C:\Users\张三\katago\model.bin.gz; no encoding dependency.
const GBK_MODEL_LINE: &[u8] = b"Loading model C:\\Users\\\xD5\xC5\xC8\xFD\\katago\\model.bin.gz\n";

pub(crate) fn startup(options: &Options) -> Result<(), RunError> {
    if options.startup_gbk_path {
        let mut stderr = io::stderr().lock();
        stderr.write_all(GBK_MODEL_LINE)?;
        stderr.flush()?;
    }
    Ok(())
}

pub(crate) fn before_query(
    options: &Options,
    query_number: u64,
    output: &mut impl Write,
) -> Result<(), RunError> {
    if options.stdout_line_before_query == Some(query_number)
        && let Some(bytes) = options.stdout_line_bytes
    {
        output.write_all(PADDED_WARNING)?;
        repeat_byte(
            output,
            b' ',
            bytes.saturating_sub(PADDED_WARNING.len() as u64),
        )?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    if options.stdout_invalid_before_query == Some(query_number) {
        output.write_all(b"\xFF\n")?;
        output.flush()?;
    }
    if query_number == 1
        && let Some(bytes) = options.stderr_invalid_flood_bytes
    {
        // Intentionally synchronous and before queuing the reply: if the parent
        // stops draining at the invalid byte, the pipe fills and this blocks.
        let mut stderr = io::stderr().lock();
        stderr.write_all(b"\xFF\n")?;
        stderr.flush()?;
        repeat_byte(&mut stderr, b'f', bytes)?;
        stderr.write_all(b"\nfake stderr flood complete\n")?;
        stderr.flush()?;
    }
    Ok(())
}

fn repeat_byte(output: &mut impl Write, byte: u8, mut remaining: u64) -> io::Result<()> {
    let chunk = [byte; 8192];
    while remaining > 0 {
        let count = remaining.min(chunk.len() as u64) as usize;
        output.write_all(&chunk[..count])?;
        remaining -= count as u64;
    }
    Ok(())
}
