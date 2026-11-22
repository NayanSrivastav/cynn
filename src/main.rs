use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "git-lineage")]
#[command(bin_name = "git-lineage")]
#[command(author, version, about = "The Codebase Archaeology CLI & Agent Forensic Primitive", long_about = None)]
struct Cli {
    /// Target file and line (e.g., path/to/file.rs:42 or path/to/file.rs:40-60)
    #[arg(value_name = "TARGET")]
    target: PathBuf,

    /// Output clean GitHub-flavored Markdown
    #[arg(long)]
    md: bool,

    /// Output structured JSON for AI agents and tooling
    #[arg(long)]
    json: bool,

    /// Run in recursive directory analysis mode
    #[arg(short, long)]
    recursive: bool,

    /// Launch interactive terminal explorer (TUI)
    #[arg(short, long)]
    interactive: bool,
}

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();

    println!("git-lineage v{}", env!("CARGO_PKG_VERSION"));
    println!("Analyzing target: {:?}", args.target);

    Ok(())
}
