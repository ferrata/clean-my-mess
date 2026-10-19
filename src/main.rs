use std::{env, io, path::PathBuf};

fn main() -> io::Result<()> {
    let root = match env::args().nth(1) {
        Some(arg) => PathBuf::from(arg),
        None => env::current_dir()?,
    };
    clean_my_mess::run_in_terminal(root)
}
