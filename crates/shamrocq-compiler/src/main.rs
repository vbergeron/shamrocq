use std::path::PathBuf;

use clap::Parser;
use shamrocq_compiler::pass::PassConfig;

/// Compile Scheme files into shamrocq bytecode for bare-metal execution.
///
/// Individual compiler passes can be toggled with --pass:NAME=yes/no.
/// Use --list-passes to see available pass names.
#[derive(Parser)]
#[command(name = "shamrocq-compiler", version)]
struct Cli {
    /// Output directory for generated files
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// Embed constructor tag names in the bytecode blob
    #[arg(long)]
    embed_tags: bool,

    /// Maximum number of optimization pass iterations (0 = no optimization)
    #[arg(long, default_value_t = shamrocq_compiler::DEFAULT_MAX_PASS_ITERATIONS)]
    max_pass_iterations: usize,

    /// List available compiler passes and exit
    #[arg(long)]
    list_passes: bool,

    /// Scheme source files to compile
    files: Vec<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let raw_args: Vec<String> = std::env::args().collect();

    let mut pass_config = PassConfig::new();
    let mut clap_args = Vec::new();
    for arg in &raw_args {
        if arg.starts_with("--pass:") {
            let (name, enabled) = PassConfig::parse_flag(arg)?;
            pass_config.set(&name, enabled);
        } else {
            clap_args.push(arg.clone());
        }
    }

    let cli = Cli::parse_from(&clap_args);

    if cli.list_passes {
        for name in PassConfig::all_pass_names() {
            let status = if pass_config.is_enabled(name) { "on" } else { "off" };
            eprintln!("  {:<30} {}", name, status);
        }
        return Ok(());
    }

    if cli.files.is_empty() {
        return Err("no input files".into());
    }

    let sources: Vec<String> = cli
        .files
        .iter()
        .map(|p| {
            std::fs::read_to_string(p)
                .map_err(|e| format!("cannot read {}: {}", p.display(), e))
        })
        .collect::<Result<_, _>>()?;
    let refs: Vec<&str> = sources.iter().map(|s| s.as_str()).collect();

    let (mut prog, tags) = shamrocq_compiler::compile_sources_with_config(
        &refs,
        cli.max_pass_iterations,
        &pass_config,
    )?;
    if cli.embed_tags {
        prog.header.tags = tags.entries().into_iter().map(|(name, _)| name).collect();
    }

    prog.emit_artifacts(&tags, &cli.output)?;

    eprintln!(
        "compiled {} globals, {} ctors, {} foreign fns, {} bytes of bytecode from {} files",
        prog.header.n_globals,
        tags.entries().len(),
        prog.foreign_fns.len(),
        prog.serialize().len(),
        cli.files.len(),
    );

    Ok(())
}
