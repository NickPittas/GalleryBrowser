//! Command-line parsing (`gallerybrowser [--help] [--version] [--] [PATH...]`). Pure: no I/O, so it
//! is unit-testable. Relative paths are made absolute against the caller's working directory so a
//! path forwarded to an already-running instance (whose cwd differs) still means the same thing.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Cli {
    /// Start (or forward to) the app with these absolute paths; may be empty.
    Run(Vec<PathBuf>),
    Help,
    Version,
}

pub const HELP: &str = "\
GalleryBrowser - asset browser for video, images and image sequences

USAGE:
    gallerybrowser [OPTIONS] [PATH...]

ARGS:
    PATH...    Folders to browse or files to reveal. A folder is opened; a file opens its
               folder, selects it and (for media) shows it in the viewer. If GalleryBrowser
               is already running the paths are handed to that window instead.

OPTIONS:
    -h, --help       Print this help and exit
    -V, --version    Print the version and exit
    --               Treat every following argument as a path
";

/// Parses `args` (without the program name). `--help`/`--version` win over everything else.
/// An unknown option is an error message suitable for printing to stderr.
pub fn parse<I>(args: I, cwd: &Path) -> Result<Cli, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut paths = Vec::new();
    let mut help = false;
    let mut version = false;
    let mut literal = false;

    for arg in args {
        if !literal {
            match arg.to_str() {
                Some("--") => {
                    literal = true;
                    continue;
                }
                Some("-h" | "--help") => {
                    help = true;
                    continue;
                }
                Some("-V" | "--version") => {
                    version = true;
                    continue;
                }
                Some(s) if s.len() > 1 && s.starts_with('-') => {
                    return Err(format!("unknown option '{s}' (try --help)"));
                }
                _ => {}
            }
        }
        if arg.is_empty() {
            continue;
        }
        paths.push(absolutize(Path::new(&arg), cwd));
    }

    if help {
        Ok(Cli::Help)
    } else if version {
        Ok(Cli::Version)
    } else {
        Ok(Cli::Run(paths))
    }
}

/// `cwd`-relative → absolute, with `.` components dropped (no filesystem access, no symlink
/// resolution: image-sequence patterns such as `shot_####.exr` do not exist on disk).
fn absolutize(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.components().collect()
    } else {
        cwd.join(path).components().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_args_runs_with_no_paths() {
        assert_eq!(parse(os(&[]), Path::new("/w")), Ok(Cli::Run(vec![])));
    }

    #[test]
    fn relative_paths_are_made_absolute_and_normalised() {
        let cli = parse(
            os(&["shots", "./a/b.mov", "/abs/x.png"]),
            Path::new("/work"),
        )
        .unwrap();
        assert_eq!(
            cli,
            Cli::Run(vec![
                PathBuf::from("/work/shots"),
                PathBuf::from("/work/a/b.mov"),
                PathBuf::from("/abs/x.png"),
            ])
        );
    }

    #[test]
    fn help_and_version_flags() {
        assert_eq!(parse(os(&["x", "--help"]), Path::new("/w")), Ok(Cli::Help));
        assert_eq!(parse(os(&["-V"]), Path::new("/w")), Ok(Cli::Version));
        assert_eq!(
            parse(os(&["--version", "-h"]), Path::new("/w")),
            Ok(Cli::Help)
        );
    }

    #[test]
    fn unknown_option_is_an_error_but_double_dash_allows_dashed_names() {
        assert!(parse(os(&["--bogus"]), Path::new("/w")).is_err());
        assert_eq!(
            parse(os(&["--", "-odd.mov", "--help"]), Path::new("/w")),
            Ok(Cli::Run(vec![
                PathBuf::from("/w/-odd.mov"),
                PathBuf::from("/w/--help")
            ]))
        );
    }

    #[test]
    fn lone_dash_and_empty_args() {
        assert_eq!(
            parse(os(&["-", ""]), Path::new("/w")),
            Ok(Cli::Run(vec![PathBuf::from("/w/-")]))
        );
    }
}
