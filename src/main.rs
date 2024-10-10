use std::fs::File;

use pgp::crypto::hash::HashAlgorithm;
use pgp::types::PublicKeyTrait;
use pgp::types::SecretKeyTrait;
use rand::rngs::OsRng;
use wolfpack::deb;
use wolfpack::pkg;
use wolfpack::pkg::CompactManifest;
use wolfpack::sign::PgpCleartextSigner;
use wolfpack::PkgPackage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (secret_key, public_key) = generate_secret_key()?;
    println!("Key id: {:x}", public_key.key_id());
    println!(
        "Fingerprint: {}",
        hex::encode(public_key.fingerprint().as_bytes())
    );
    let control_file = std::env::args().nth(1).unwrap();
    let directory = std::env::args().nth(2).unwrap();
    let control_data: deb::Package = std::fs::read_to_string(control_file)?.parse()?;
    eprintln!("{}", control_data);
    let (deb_signing_key, deb_verifying_key) =
        deb::SigningKey::generate("deb-key-id".into()).unwrap();
    let deb_signer = deb::PackageSigner::new(deb_signing_key);
    let deb_verifier = deb::PackageVerifier::new(deb_verifying_key);
    control_data.write(directory, File::create("test.deb")?, &deb_signer)?;
    let manifest: CompactManifest =
        std::fs::read_to_string("freebsd/+COMPACT_MANIFEST")?.parse()?;
    PkgPackage::new(manifest, "freebsd/root".into()).build(File::create("test.pkg")?)?;
    {
        let packages = pkg::Packages::new(["test.pkg"])?;
        packages.build(File::create("packagesite.pkg")?, &secret_key)?;
    }
    let deb_release_signer = PgpCleartextSigner::new(secret_key.clone());
    deb::Repository::write(
        "repo",
        "test".parse()?,
        ["test.deb"],
        &deb_verifier,
        &deb_release_signer,
    )?;
    // TODO ipk has its own whitelist of fields, see opkg.py
    // TODO freebsd http://pkg.freebsd.org/FreeBSD:15:amd64/base_latest/
    //let ipk_signing_key = ksign::SigningKey::generate(None);
    //ipk_signing_key
    //    .sign(packages_string.as_bytes())
    //    .write_to_file(Path::new("Packages.sig"))?;
    Ok(())
}

fn generate_secret_key() -> Result<(pgp::SignedSecretKey, pgp::SignedPublicKey), pgp::errors::Error>
{
    use pgp::composed::*;
    use pgp::crypto::sym::SymmetricKeyAlgorithm;
    use pgp::types::CompressionAlgorithm;
    let mut key_params = SecretKeyParamsBuilder::default();
    key_params
        .key_type(KeyType::EdDSALegacy)
        .can_certify(false)
        .can_sign(true)
        .primary_user_id("none".into())
        .preferred_symmetric_algorithms([SymmetricKeyAlgorithm::AES256].as_slice().into())
        .preferred_hash_algorithms([HashAlgorithm::SHA2_512].as_slice().into())
        .preferred_compression_algorithms([CompressionAlgorithm::ZLIB].as_slice().into());
    let secret_key_params = key_params
        .build()
        .expect("Must be able to create secret key params");
    let secret_key = secret_key_params
        .generate(OsRng)
        .expect("Failed to generate a plain key.");
    let signed_secret_key = secret_key
        .sign(OsRng, String::new)
        .expect("Must be able to sign its own metadata");
    let signed_public_key = signed_secret_key
        .public_key()
        .sign(OsRng, &signed_secret_key, String::new)
        .unwrap();
    Ok((signed_secret_key, signed_public_key))
}
