use std::io;

fn main() -> io::Result<()> {
    match model_select::run_selector(false) {
        Ok(outcome) => {
            println!("\r\n   {}\r\n", outcome.summary);
            Ok(())
        }
        Err(e) => {
            eprintln!("\x1b[1;31m[error] {}\x1b[0m", e);
            Err(e)
        }
    }
}
