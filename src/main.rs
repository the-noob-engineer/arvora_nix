use goblin::{Object, error};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;

fn goblin_runner(file_path: &PathBuf) -> Result<(), Box<dyn std::error::Error + 'static>> {
    let file_data = fs::read(file_path)?;
    println!("{:?}", file_data);

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if !args.len() > 1 {
        println!("No command-line arguments provided (except the executable path).");
        exit(0)
    }

    let mut file = String::from("");
    let user_args: Vec<&String> = args.iter().skip(1).collect();

    file = String::from(user_args[0]);

    let exe_path = Path::new(&file);
    println!("{:?}", exe_path);

    let absolute_path = exe_path.canonicalize().unwrap();
    println!("Absolute path: {:?}", &absolute_path);

    goblin_runner(&absolute_path);
}
