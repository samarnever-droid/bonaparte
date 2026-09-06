//! Standalone executable for the Bonaparte MCP headless server.

use std::io::{self, BufReader, BufWriter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let reader = BufReader::new(stdin.lock());
    let writer = BufWriter::new(stdout.lock());
    bonaparte_mcp::run_stdio(reader, writer)?;
    Ok(())
}
