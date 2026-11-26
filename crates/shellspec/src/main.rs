//! The `shellspec` command line. Parser, help and process adapter come from the generated
//! `shellspec-cli-contract` (`ess generate cli` over `spec/cli.yaml`); this file supplies the
//! handler behind each command.

use std::ffi::OsString;
use std::io::{ErrorKind, Write};

use cli_contract::{Handler, HandlerReply, Invocation};
use serde_json::json;
use shellspec::cli;

const VERSION_LINE: &str = concat!("shellspec ", env!("CARGO_PKG_VERSION"), "\n");

struct Commands;

impl Handler for Commands {
    fn call(&mut self, inv: &Invocation<'_>) -> HandlerReply {
        match cli::call(inv.callable, &inv.input) {
            Ok(Some(v)) => HandlerReply::Success(v),
            Ok(None) => HandlerReply::Unavailable,
            Err((code, detail)) => HandlerReply::Error {
                code,
                data: json!({ "detail": detail }),
            },
        }
    }
}

fn main() {
    let args: Vec<OsString> = std::env::args_os().collect();
    if args.len() == 2 && args[1] == "--version" {
        std::process::exit(emit(VERSION_LINE, "", 0));
    }
    let output = cli_contract::run(args, &mut cli_contract::OsSources, &mut Commands, None);
    std::process::exit(emit(&output.stdout, &output.stderr, output.exit_code));
}

fn emit(stdout: &str, stderr: &str, exit_code: i32) -> i32 {
    let mut out = std::io::stdout().lock();
    let written = out.write_all(stdout.as_bytes()).and_then(|()| out.flush());
    let mut err = std::io::stderr().lock();
    let _ = err.write_all(stderr.as_bytes());
    match written {
        Ok(()) => exit_code,
        Err(error) if error.kind() == ErrorKind::BrokenPipe => exit_code,
        Err(error) => {
            let _ = writeln!(err, "shellspec: standard output: {error}");
            if exit_code == 0 { 1 } else { exit_code }
        }
    }
}
