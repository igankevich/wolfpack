use std::fs::File;

use wolfpack::deb::ControlData;
use wolfpack::deb::Package;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let control_file = std::env::args().nth(1).unwrap();
    let directory = std::env::args().nth(2).unwrap();
    let control_data: ControlData = std::fs::read_to_string(control_file)?.parse()?;
    let pkg = Package::new(control_data, directory.into());
    pkg.build(File::create("test.deb")?)?;
    Ok(())
}

/*
fn main() -> Result<(), Box<dyn std::error::Error>> {
use std::io::BufRead;
use std::io::BufReader;
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
*/
