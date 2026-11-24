use crate::{
    core::{GenesisAnalyzer, Target},
    git::GitRepo,
    output::{render_ansi, render_json, render_markdown},
};
use clap::Parser;
use std::path::Path;

#[derive(Parser, Debug)]
#[command(name = "cynn")]
#[command(
    author = "Nayan Srivastava <nayansrivastav@yahoo.in>",
    version,
    about = "The Codebase Archaeology CLI & AI Agent Forensic Primitive",
    long_about = "cynn: Reconstruct the true lineage, rationale, and evolution of any line of code.\n\n\
MULTI-PLATFORM SUPPORT:\n  \
• GitHub: Deep PR extraction via gh CLI & GraphQL/REST API (merged PRs, discussions, inline reviews, linked issues).\n  \
• GitLab: Merge Request resolution via glab CLI and commit trailers ('See merge request !123').\n  \
• Bitbucket: Pull Request extraction via merge trailers ('Merged in ... (pull request #123)').\n  \
• Generic Git: Universal offline commit trailer resolution for air-gapped or self-hosted servers.\n\n\
AUTHENTICATION & TOKENS:\n  \
• GitHub: export GITHUB_TOKEN=\"<token>\" (or GH_TOKEN), or log in via 'gh auth login'\n  \
• GitLab: export GITLAB_TOKEN=\"<token>\" (or GL_TOKEN), or log in via 'glab auth login'\n  \
• Bitbucket: export BITBUCKET_TOKEN=\"<token>\"\n  \
• If no token or CLI is configured, cynn seamlessly falls back to offline Git trailers.\n\n\
CONFIDENCE TIERS & VERIFICATION:\n  \
• High: Substantive PR/MR body (>20 chars) or verified technical reviewer decision.\n  \
• Medium: Substantive commit description (>20 chars) or verified multi-origin composition.\n  \
• Low: PR/MR title or single-line commit subject only (no extended rationale recorded).\n  \
• None: Zero rationale recorded in history (strictly unhallucinated).\n  \
• Reference: https://nayansrivastav.github.io/cynn/#confidence-tiers\n\n\
AGENT FORENSIC ENGINE:\n  \
• Separates objective historical facts (introducing commit, blame, citations) from inferred intent.\n  \
• Outputs structured, hallucination-resistant JSON schema for autonomous coding agents.\n  \
• Multi-origin dissection traces each sub-slice of a block to its exact introducing commit."
)]
pub struct Cli {
    /// Target file and line or function (e.g., path/to/file.rs:42, file.rs:40-60, file.rs:functionName, or -L :func:file)
    #[arg(value_name = "TARGET")]
    pub target: String,

    /// Working directory / repository root path (defaults to current directory)
    #[arg(short = 'C', long = "repo", value_name = "PATH")]
    pub repo: Option<std::path::PathBuf>,

    /// Output clean GitHub-flavored Markdown
    #[arg(long)]
    pub md: bool,

    /// Output structured JSON for AI agents and tooling
    #[arg(long)]
    pub json: bool,

    /// Include the unified diff patch from the introducing genesis commit
    #[arg(short = 'p', long = "diff")]
    pub diff: bool,
}

pub fn run_cli() -> anyhow::Result<()> {
    let args = Cli::parse();

    // 1. Parse target
    let target: Target = args
        .target
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid target '{e}'"))?;

    // 2. Discover Git repository
    let repo_path = args.repo.as_deref().unwrap_or(Path::new("."));
    let repo = GitRepo::discover(repo_path).map_err(|e| anyhow::anyhow!("{e}"))?;

    // 3. Analyze genesis provenance
    let analyzer = GenesisAnalyzer::new(&repo);
    let dossier = analyzer
        .analyze_with_options(&target, args.diff)
        .map_err(|e| anyhow::anyhow!("Analysis failed: {e}"))?;

    // 4. Render output based on flags
    if args.json {
        let json_out = render_json(&dossier).map_err(|e| anyhow::anyhow!("{e}"))?;
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
