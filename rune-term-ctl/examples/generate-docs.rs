//! Render the CLI reference from the spec the derives wrote.
//!
//! `WINNOW_ARGS_SPEC=/abs/target/spec cargo check -p rune-term-ctl` writes the
//! spec; this example turns it into man pages and Markdown reference pages.

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec = Path::new("target/spec");
    let catalog = winnow_args_spec::Catalog::load(spec)?;
    // The workspace also derives the app's CLI; name the ctl root explicitly.
    let command = catalog.stitch("Cli")?;

    std::fs::create_dir_all("man")?;
    for (name, page) in
        winnow_args_man::Manual::default().render_pages(&command, "rune-term-ctl")?
    {
        std::fs::write(Path::new("man").join(name), page)?;
    }

    std::fs::create_dir_all("docs/cli")?;
    for (name, page) in winnow_args_markdown::render_pages(&command, "rune-term-ctl")? {
        std::fs::write(Path::new("docs/cli").join(name), page)?;
    }
    Ok(())
}
