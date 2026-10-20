use celldiff::{
    AppError,
    compare::{compare, compare_all},
    console::{AddressStyle, TextReport},
    encoding::{InputEncoding, list_encodings},
    input::CsvInput,
    output::{Report, validate_paths},
};
use clap::{CommandFactory, FromArgMatches, Parser, error::ErrorKind, parser::ValueSource};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, after_help = "Encoding examples: utf-8, cp932, utf-16le. Use --list-encodings for all supported names and aliases.")]
struct Args {
    /// Left CSV, used as the comparison baseline (first record is included)
    #[arg(required_unless_present = "list_encodings")]
    left: Option<PathBuf>,
    /// Right CSV, compared against the left input
    #[arg(required_unless_present = "list_encodings")]
    right: Option<PathBuf>,
    /// Output XLSX report
    #[arg(short, long, value_name = "FILE", default_value = "diff.xlsx")]
    output: PathBuf,
    /// Replace an existing report (input files are always protected)
    #[arg(long)]
    force: bool,
    /// Encoding of the left CSV (no automatic detection)
    #[arg(
        long = "encoding-left",
        visible_alias = "el",
        default_value = "utf-8",
        value_name = "ENCODING"
    )]
    encoding_left: InputEncoding,
    /// Encoding of the right CSV (no automatic detection)
    #[arg(
        long = "encoding-right",
        visible_alias = "er",
        default_value = "utf-8",
        value_name = "ENCODING"
    )]
    encoding_right: InputEncoding,
    /// List supported encoding names and aliases, without reading files
    #[arg(long)]
    list_encodings: bool,
    /// Print changed cells to stdout without creating an XLSX file
    #[arg(long, conflicts_with_all = ["output", "force", "list_encodings"])]
    text: bool,
    /// Print one changed cell per line (requires --text)
    #[arg(long, requires = "text", conflicts_with = "list_encodings")]
    oneline: bool,
    /// Cell address style for --text
    #[arg(long, value_enum, default_value = "a1", value_name = "STYLE")]
    address: AddressStyle,
}

fn run(args: Args) -> Result<bool, AppError> {
    if args.list_encodings {
        list_encodings();
        return Ok(false);
    }
    // clap requires these unless --list-encodings is present.
    let left_path = args.left.as_ref().expect("left required by clap");
    let right_path = args.right.as_ref().expect("right required by clap");
    let output = &args.output;
    let mut left = CsvInput::open_encoded(left_path, args.encoding_left)?;
    let mut right = CsvInput::open_encoded(right_path, args.encoding_right)?;
    if args.text {
        let stdout = std::io::stdout();
        let mut report = TextReport::new(stdout.lock(), args.oneline, args.address);
        let summary = compare(&mut left, &mut right, |cell| {
            report.write(cell)?;
            Ok(())
        })?;
        report.finish(summary)?;
        return Ok(summary.changed_cells != 0);
    }
    validate_paths(left_path, right_path, output, args.force)?;
    let mut report = Report::new()?;
    let summary = compare_all(&mut left, &mut right, |d| report.write(d))?;
    report.save(
        output,
        args.force,
        summary,
        left_path,
        right_path,
        (args.encoding_left.name(), args.encoding_right.name()),
    )?;
    println!(
        "changed_cells={} changed_records={} modified={} added={} removed={} output={}",
        summary.changed_cells,
        summary.changed_records,
        summary.modified,
        summary.added,
        summary.removed,
        output.display()
    );
    Ok(summary.changed_cells != 0)
}

fn parse_args() -> Args {
    let mut command = Args::command();
    let matches = command.clone().get_matches();
    // Defaults are not explicit options: ordinary XLSX runs may keep address=a1.
    if matches.value_source("address") == Some(ValueSource::CommandLine)
        && !matches.get_flag("text")
    {
        command
            .error(
                ErrorKind::ArgumentConflict,
                "--address can only be used with --text",
            )
            .exit();
    }
    Args::from_arg_matches(&matches).unwrap_or_else(|error| error.exit())
}

fn main() -> ExitCode {
    match run(parse_args()) {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(1),
        Err(error) => {
            eprintln!("celldiff: {error}");
            ExitCode::from(2)
        }
    }
}
