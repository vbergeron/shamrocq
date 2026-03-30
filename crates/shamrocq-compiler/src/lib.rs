pub mod bytecode;
pub mod codegen;
pub mod desugar;
pub mod parser;
pub mod resolve;

use codegen::CompiledProgram;
use resolve::TagTable;

pub fn compile_sources(sources: &[&str]) -> Result<(CompiledProgram, TagTable), String> {
    let mut all_sexps = Vec::new();
    for src in sources {
        all_sexps.extend(parser::parse(src).map_err(|e| e.to_string())?);
    }
    let defs = desugar::desugar_program(&all_sexps)?;
    let mut tags = TagTable::new();
    let mut globals = resolve::GlobalTable::new();
    let rdefs = resolve::resolve_program(&defs, &mut tags, &mut globals)?;
    Ok((codegen::compile_program(&rdefs), tags))
}

pub fn compile_to_dir(
    sources: &[&str],
    dir: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let (prog, tags) = compile_sources(sources)?;
    prog.emit_artifacts(&tags, dir)?;
    Ok(())
}
