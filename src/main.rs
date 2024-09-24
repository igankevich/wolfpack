use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;

use wolfpack::deb::ControlData;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(std::env::args().nth(1).unwrap())?;
    let reader = BufReader::new(file);
    let mut string = String::with_capacity(4096);
    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            let control: ControlData = string.parse()?;
            println!("{}", control);
            string.clear();
        } else {
            string.push_str(&line);
            string.push('\n');
        }
    }
    if !string.is_empty() {
        let control: ControlData = string.parse()?;
        println!("{}", control);
    }
    Ok(())
}
