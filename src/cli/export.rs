//! `moot export` — write a captured meeting to files on disk (SPEC §5.5).

use std::path::PathBuf;

use clap::{Args as ClapArgs, ValueEnum};

use super::Context;
use crate::bundle::{self, Bundle};
use crate::error::{Error, Result};
use crate::store::{Store, resolve_path};

#[derive(Debug, Copy, Clone, ValueEnum)]
pub enum Format {
    Jsonl,
    Md,
    All,
}

impl From<Format> for bundle::Format {
    fn from(f: Format) -> Self {
        match f {
            Format::Jsonl => bundle::Format::Jsonl,
            Format::Md => bundle::Format::Md,
            Format::All => bundle::Format::All,
        }
    }
}

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Meeting id or slug.
    pub target: String,

    /// Output directory. Use `-` to stream a tar archive to stdout.
    #[arg(long, value_name = "DIR|-")]
    pub out: Option<String>,

    /// What to write.
    #[arg(long, value_enum, default_value_t = Format::All)]
    pub format: Format,

    /// Overwrite an existing target directory.
    #[arg(long)]
    pub force: bool,
}

pub async fn execute(ctx: &Context, args: Args) -> Result<()> {
    let store_path = resolve_path(ctx.db.as_deref())?;
    let store = Store::open(&store_path)?;
    let meeting = store.require_meeting(&args.target)?;
    let tags = store.tags_for(&meeting.id)?;
    let bundle = Bundle::build(&meeting, &tags, args.format.into())?;

    if let Some(out) = &args.out {
        if out == "-" {
            if args.force {
                return Err(Error::Cli(
                    "--force cannot be combined with --out -".into(),
                ));
            }
            // Stream tar to stdout.
            let stdout = std::io::stdout();
            let lock = stdout.lock();
            bundle.write_tar(lock)?;
            return Ok(());
        }
        let path: PathBuf = out.into();
        let target = bundle.write_to_dir(&path, args.force)?;
        announce(ctx, &meeting.slug, &target);
        return Ok(());
    }

    // Default: current working directory.
    let cwd = std::env::current_dir().map_err(|e| Error::Fs(format!("cwd: {e}")))?;
    let target = bundle.write_to_dir(&cwd, args.force)?;
    announce(ctx, &meeting.slug, &target);
    Ok(())
}

fn announce(ctx: &Context, slug: &str, target: &std::path::Path) {
    if ctx.json {
        let payload = serde_json::json!({"slug": slug, "path": target.display().to_string()});
        println!("{payload}");
    } else {
        println!("Wrote {} to {}", slug, target.display());
    }
}
