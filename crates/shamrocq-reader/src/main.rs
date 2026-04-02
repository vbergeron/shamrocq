mod color;
mod disasm;
mod dump;
mod header;
mod printer;
mod scan;
mod style;
mod tui;
mod util;

use std::io::IsTerminal;
use std::path::PathBuf;

use clap::Parser;

use color::C;

/// Read and disassemble a shamrocq bytecode blob.
#[derive(Parser)]
#[command(name = "shamrocq-reader", version)]
struct Cli {
    /// Bytecode file to disassemble (e.g. bytecode.bin)
    file: PathBuf,

    /// Color output mode
    #[arg(long, value_enum, default_value = "auto")]
    color: ColorMode,

    /// Interactive TUI mode
    #[arg(short, long)]
    interactive: bool,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let use_color = match cli.color {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => std::io::stdout().is_terminal(),
    };
    let c = if use_color { C::on() } else { C::off() };
    let blob = std::fs::read(&cli.file)
        .map_err(|e| format!("cannot read {}: {}", cli.file.display(), e))?;

    if blob[0..4] == shamrocq_bytecode::DUMP_MAGIC {
        dump::display_dump(&blob, &c).map_err(|e| format!("dump error: {}", e))?;
    } else if blob[0..4] == shamrocq_bytecode::MAGIC {
        let fname = cli.file.file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| cli.file.display().to_string());
        let d = disasm::disassemble(&blob, &fname).map_err(|e| format!("disassembly error: {}", e))?;
        if cli.interactive {
            tui::run(d)?;
        } else {
            printer::print_disassembly(&d, &c);
        }
    } else {
        return Err(format!("unrecognized magic: {:?}", &blob[0..4]).into());
    }
    Ok(())
}
