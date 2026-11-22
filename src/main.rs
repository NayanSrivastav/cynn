use clap::Parser;
use git_lineage::{
    output::{render_ansi, render_json, render_markdown},
    GenesisAnalyzer, GitRepo, Target,
};
use std::path::Path;

#[derive(Parser, Debug)]
#[command(name = "git-lineage")]
#[command(bin_name = "git-lineage")]
#[command(author, version, about = "The Codebase Archaeology CLI & Agent Forensic Primitive", long_about = None)]
struct Cli {
    /// Target file and line (e.g., path/to/file.rs:42 or path/to/file.rs:40-60)
    #[arg(value_name = "TARGET")]
    target: String,

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

    // 1. Parse target
    let target: Target = args
        .target
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid target '{e}'"))?;

    // 2. Discover Git repository
    let repo = GitRepo::discover(Path::new("."))
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // 3. Analyze genesis provenance
    let analyzer = GenesisAnalyzer::new(&repo);
    let dossier = analyzer
        .analyze(&target)
        .map_err(|e| anyhow::anyhow!("Analysis failed: {e}"))?;

    // 4. Render output based on flags
    if args.json {
        let json_out = render_json(&dossier)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        println!("{json_out}");
    } else if args.md {
        let md_out = render_markdown(&dossier);
        println!("{md_out}");
    } else {
        let ansi_out = render_ansi(&dossier);
        print!("{ansi_out}");
    }

    Ok(())
}
