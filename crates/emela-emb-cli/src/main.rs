mod args;
mod build;
mod output;

use std::process::ExitCode;

fn main() -> ExitCode {
    match args::parse(std::env::args_os().skip(1)) {
        Ok(args::Command::Help) => {
            println!("{}", args::USAGE);
            ExitCode::SUCCESS
        }
        Ok(args::Command::Build {
            inputs,
            out_dir,
            entry,
            options,
        }) => match build::run(&inputs, &out_dir, entry.as_deref(), &options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("{message}");
                ExitCode::FAILURE
            }
        },
        Err(message) => {
            eprintln!("emela-emb: {message}\n\n{}", args::USAGE);
            ExitCode::from(2)
        }
    }
}
