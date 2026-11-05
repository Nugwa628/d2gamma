//! Publication integrity and persistent keys, using synthetic data only.
use d2_modkit::{publish, MemoryPack, ModError};

fn pack() -> MemoryPack {
    MemoryPack { files: [
        ("mod.toml".into(), b"id = 'test-mod'\nname = 'Test Mod'\nversion = '1.0.0'\nauthor = 'A'\ndescription = ''\napi = 1\nkind = 'full'\n".to_vec()),
        ("files/rules.toml".into(), b"# synthetic".to_vec()),
    ].into() }
}

#[test]
fn signed_round_trip_and_tampering() {
    let path = std::env::temp_dir().join(format!(
        "d2gamma-key-test-{}/author.pk8",
        std::process::id()
    ));
    let key = publish::author_key(&path).unwrap();
    let before = std::fs::read(&path).unwrap();
    let key_again = publish::author_key(&path).unwrap();
    assert_eq!(before, std::fs::read(&path).unwrap());
    let mut a = pack();
    let hash = publish::seal(&mut a, &key).unwrap();
    assert_eq!(publish::verify(&a).unwrap().content_hash, hash);
    let mut b = pack();
    publish::seal(&mut b, &key_again).unwrap();
    assert_eq!(a, b);
    b.files.get_mut("files/rules.toml").unwrap()[0] ^= 1;
    assert!(matches!(
        publish::verify(&b),
        Err(ModError::HashMismatch { .. })
    ));
    b = a.clone();
    let record = b.files.get_mut(publish::RECORD).unwrap();
    let text = String::from_utf8(record.clone()).unwrap();
    let key_start = text.find("ed25519:").unwrap() + 8;
    record[key_start] = if record[key_start] == b'0' {
        b'1'
    } else {
        b'0'
    };
    assert!(matches!(publish::verify(&b), Err(ModError::BadSignature)));
    // The API pin belongs to the signed manifest, too.
    b = a.clone();
    let changed = String::from_utf8(b.files["mod.toml"].clone())
        .unwrap()
        .replace("api = 1", "api = 2");
    b.files.insert("mod.toml".into(), changed.into_bytes());
    assert!(matches!(
        publish::verify(&b),
        Err(ModError::HashMismatch { .. })
    ));
    assert!(publish::seal(&mut a, &key).is_err());
    // Review metadata can be added without invalidating the author signature.
    let review = d2_modkit::PackSignature::sign(&hash, &key_again).to_text();
    let record = a.files.get_mut(publish::RECORD).unwrap();
    record.extend_from_slice(format!("reviewed_by_d2gamma = '{review}'\n").as_bytes());
    publish::verify(&a).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
