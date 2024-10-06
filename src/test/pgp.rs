use pgp::composed::*;
use pgp::types::SecretKeyTrait;
use pgp::SignedPublicKey;
use pgp::SignedSecretKey;
use rand::rngs::OsRng;

pub fn pgp_keys(key_type: KeyType) -> (SignedSecretKey, SignedPublicKey) {
    let mut key_params = SecretKeyParamsBuilder::default();
    key_params
        .key_type(key_type)
        .can_encrypt(false)
        .can_certify(false)
        .can_sign(true)
        .primary_user_id("wolfpack test id".into());
    let secret_key_params = key_params.build().unwrap();
    let secret_key = secret_key_params.generate(OsRng).unwrap();
    let signed_secret_key = secret_key.sign(OsRng, String::new).unwrap();
    let signed_public_key = signed_secret_key
        .public_key()
        .sign(OsRng, &signed_secret_key, String::new)
        .unwrap();
    (signed_secret_key, signed_public_key)
}
