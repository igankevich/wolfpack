use std::collections::HashSet;
use std::fs::File;

use pgp::composed::cleartext::CleartextSignedMessage;
use pgp::types::PublicKeyTrait;
use pgp::types::SecretKeyTrait;
use rand::rngs::OsRng;
use wolfpack::deb::ControlData;
use wolfpack::deb::Packages;
use wolfpack::deb::Release;
use wolfpack::deb::SimpleValue;
use wolfpack::DebPackage;
use wolfpack::IpkPackage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let control_file = std::env::args().nth(1).unwrap();
    let directory = std::env::args().nth(2).unwrap();
    let control_data: ControlData = std::fs::read_to_string(control_file)?.parse()?;
    let deb = DebPackage::new(control_data.clone(), directory.clone().into());
    deb.build(File::create("test.deb")?)?;
    IpkPackage::new(control_data, directory.into()).build(File::create("test.ipk")?)?;
    let packages = Packages::new(["."])?;
    print!("{}", packages);
    let mut architectures: HashSet<SimpleValue> = HashSet::new();
    for package in packages.into_iter() {
        architectures.insert(package.control.architecture);
    }
    let release = Release::new(".", architectures, SimpleValue::try_from("test".into())?)?;
    print!("release start `{}` release end", release);
    let secret_key = generate_secret_key()?;
    let public_key = secret_key.public_key();
    println!("Key id: {:x}", public_key.key_id());
    println!(
        "Fingerprint: {}",
        hex::encode(public_key.fingerprint().as_bytes())
    );
    // TODO trim?
    let signed_release =
        CleartextSignedMessage::sign(OsRng, release.to_string().trim(), &secret_key, || {
            String::new()
        })?;
    signed_release.to_armored_writer(&mut File::create("InRelease")?, Default::default())?;
    Ok(())
}

fn generate_secret_key() -> Result<pgp::SignedSecretKey, pgp::errors::Error> {
    use pgp::composed::*;
    use pgp::crypto::{hash::HashAlgorithm, sym::SymmetricKeyAlgorithm};
    use pgp::types::CompressionAlgorithm;
    use smallvec::smallvec;
    let mut key_params = SecretKeyParamsBuilder::default();
    key_params
        .key_type(KeyType::Rsa(2048))
        .can_certify(false)
        .can_sign(true)
        .primary_user_id("Me <me@example.com>".into())
        .preferred_symmetric_algorithms(smallvec![SymmetricKeyAlgorithm::AES256])
        .preferred_hash_algorithms(smallvec![HashAlgorithm::SHA2_512])
        .preferred_compression_algorithms(smallvec![CompressionAlgorithm::ZLIB]);
    let secret_key_params = key_params
        .build()
        .expect("Must be able to create secret key params");
    let secret_key = secret_key_params
        .generate(OsRng)
        .expect("Failed to generate a plain key.");
    let signed_secret_key = secret_key
        .sign(OsRng, || String::new())
        .expect("Must be able to sign its own metadata");
    Ok(signed_secret_key)
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
