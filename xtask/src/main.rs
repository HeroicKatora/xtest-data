use std::env;
use std::path::Path;

fn main() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Not built in the xtest-data repository");
    env::set_current_dir(repo)
        .expect("Not built in the xtest-data repository");

    let _ = Args::default();
}

struct Args {
}

impl Default for Args {
    fn default() -> Self {
        let mut args = env::args().skip(1);
        match args.next().as_ref().map(String::as_str) {
            None => panic!("No command given"),
            Some("test") => Args {},
            _ => panic!("Invalid command given"),
        }
    }
}
