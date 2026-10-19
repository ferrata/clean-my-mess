use std::{
    env, io,
    path::PathBuf,
    sync::{atomic::AtomicBool, mpsc, Arc},
    thread,
};

use clean_my_mess::scan::{self, Msg};

fn main() -> io::Result<()> {
    let root = match env::args().nth(1) {
        Some(arg) => PathBuf::from(arg),
        None => env::current_dir()?,
    };
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || scan::scan(root, tx, Arc::new(AtomicBool::new(false))));
    for msg in rx {
        match msg {
            Msg::Found(item) => println!("{:>14} {:<12} {}", item.size, item.kind, item.path.display()),
            Msg::Progress(_) => {}
            Msg::Done => break,
        }
    }
    Ok(())
}
