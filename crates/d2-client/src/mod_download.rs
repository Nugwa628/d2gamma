//! Official download consent and launcher handoff. No network or hosting policy.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

pub const SETTINGS_FILE: &str = "mod-downloads.toml";
pub const OFFER_FILE: &str = "mod-download-offer.json";
pub const REQUEST_FILE: &str = "mod-download-request.json";
pub const DEFAULT_SETTINGS: &str = "official_list_url = \"\"\n";
const MAX_OFFER: u64 = 16 * 1024;
const MAX_DOWNLOAD: u64 = 16 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub official_list_url: String,
}
impl Settings {
    pub fn parse(text: &str) -> Result<Self, String> {
        let doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
        if doc.iter().any(|(key, _)| key != "official_list_url") {
            return Err("unknown download setting".into());
        }
        let url = match doc.get("official_list_url") {
            None => "",
            Some(value) => value.as_str().ok_or("official_list_url must be text")?,
        };
        if !url.is_empty() {
            https_origin(url)?;
        }
        Ok(Self {
            official_list_url: url.into(),
        })
    }
}
fn https_origin(url: &str) -> Result<&str, String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or("official downloads require HTTPS")?;
    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty()
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-:".contains(&b))
        || url
            .bytes()
            .any(|b| b <= 32 || b == 127 || b == b'\\' || b == b'#')
    {
        return Err("invalid official download URL".into());
    }
    Ok(host)
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub id: String,
    pub version: String,
    pub author: String,
    pub bytes: u64,
    pub package_url: String,
    pub sha256: String,
    pub author_key: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Required {
    pub id: String,
    pub version: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    source_url: String,
    required: Required,
    offer: Offer,
}

/// Constructed only from a successful fetch of the exact configured URL.
/// A transport must reject redirects and limit response bytes before calling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    source_url: String,
    offer: Offer,
}
impl Prompt {
    pub fn from_official(
        settings: &Settings,
        response_url: &str,
        required: &Required,
        response: &[u8],
    ) -> Result<Self, String> {
        if settings.official_list_url.is_empty() {
            return Err("official mod downloads are disabled".into());
        }
        if response_url != settings.official_list_url || response.len() as u64 > MAX_OFFER {
            return Err("response is not from the configured official list".into());
        }
        let offer: Offer = serde_json::from_slice(response).map_err(|e| e.to_string())?;
        d2_modkit::manifest::check_id(&offer.id)?;
        d2_modkit::Version::parse(&offer.version).map_err(|e| e.to_string())?;
        if offer.id != required.id || offer.version != required.version {
            return Err("official offer differs from the required mod/version".into());
        }
        if offer.author.is_empty()
            || offer.author.chars().count() > 64
            || offer.author.chars().any(char::is_control)
        {
            return Err("invalid offer author".into());
        }
        if offer.bytes == 0 || offer.bytes > MAX_DOWNLOAD {
            return Err("invalid offer size".into());
        }
        for value in [&offer.sha256, &offer.author_key] {
            if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("invalid offer hash or signing key".into());
            }
        }
        if https_origin(&offer.package_url)? != https_origin(&settings.official_list_url)? {
            return Err("package must use the configured official origin".into());
        }
        Ok(Self {
            source_url: response_url.into(),
            offer,
        })
    }
    pub fn offer(&self) -> &Offer {
        &self.offer
    }
    pub fn text(&self) -> String {
        format!(
            "Download {} {}?\nAuthor: {}\nSize: {} bytes\nOnly continue if you trust this author.",
            self.offer.id, self.offer.version, self.offer.author, self.offer.bytes
        )
    }
    /// Called by the explicit DOWNLOAD button, never by a mismatch or Enter.
    pub fn confirm(self) -> Confirmed {
        Confirmed {
            source_url: self.source_url,
            offer: self.offer,
        }
    }
}
/// The launcher consumes only a confirmed request. Deserialization is deliberately
/// absent here so in-process callers cannot skip the consent step.
#[derive(Debug, Serialize)]
pub struct Confirmed {
    source_url: String,
    offer: Offer,
}
impl Confirmed {
    /// A one-shot handoff; never overwrites another pending consent/request.
    pub fn queue(&self, root: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join(REQUEST_FILE))
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())
    }
    /// Transport completion hook before installation. The transport must stop
    /// at the declared length, preserve HTTPS origin and refuse redirects.
    pub fn verify_package(&self, path: &Path) -> Result<(), String> {
        let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() != self.offer.bytes {
            return Err("download size changed; confirm a new official offer".into());
        }
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        if format!("{:x}", hash.finalize()) != self.offer.sha256.to_ascii_lowercase() {
            return Err("download hash differs from confirmed offer".into());
        }
        let pack = d2_native::pack::Pack::open(path).map_err(|e| e.to_string())?;
        let verified = d2_modkit::publish::verify(&pack).map_err(|e| e.to_string())?;
        let signer = verified.signer.ok_or("official download is unsigned")?;
        if d2_modkit::pack::hex(&signer) != self.offer.author_key.to_ascii_lowercase() {
            return Err("download signer differs from confirmed author".into());
        }
        let loaded = d2_native::package::open_mod(path, d2_modkit::api::Date::today())
            .map_err(|e| e.to_string())?;
        if loaded.manifest.id != self.offer.id
            || loaded.manifest.version.to_string() != self.offer.version
            || loaded.manifest.author != self.offer.author
        {
            return Err("download identity differs from confirmed offer".into());
        }
        Ok(())
    }
}
fn bounded_read(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_OFFER + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_OFFER {
        return Err("download metadata too large".into());
    }
    Ok(bytes)
}
/// Cache handoff from the launcher's official-list fetch. This code deliberately
/// supplies no list endpoint, fetch implementation, hosting or catalogue UI.
pub fn pending_under(root: &Path) -> Result<Option<Prompt>, String> {
    let settings_path = root.join(SETTINGS_FILE);
    if !settings_path.exists() {
        return Ok(None);
    }
    let text = bounded_read(&settings_path)?;
    let settings = Settings::parse(std::str::from_utf8(&text).map_err(|e| e.to_string())?)?;
    if settings.official_list_url.is_empty()
        || !root.join(OFFER_FILE).exists()
        || root.join(REQUEST_FILE).exists()
    {
        return Ok(None);
    }
    let env: Envelope = serde_json::from_slice(&bounded_read(&root.join(OFFER_FILE))?)
        .map_err(|e| e.to_string())?;
    let offer = serde_json::to_vec(&env.offer).map_err(|e| e.to_string())?;
    Prompt::from_official(&settings, &env.source_url, &env.required, &offer).map(Some)
}
pub fn pending() -> Result<Option<Prompt>, String> {
    pending_under(&crate::user_dir::d2gamma_root())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn offer() -> Offer {
        Offer {
            id: "ghost-world".into(),
            version: "1.0.0".into(),
            author: "Nugwa".into(),
            bytes: 123456,
            package_url: "https://official.example/packs/ghost.d2pack".into(),
            sha256: "a".repeat(64),
            author_key: "b".repeat(64),
        }
    }
    fn settings() -> Settings {
        Settings::parse("official_list_url='https://official.example/mods'").unwrap()
    }
    fn prompt(value: &Offer) -> Result<Prompt, String> {
        Prompt::from_official(
            &settings(),
            &settings().official_list_url,
            &Required {
                id: "ghost-world".into(),
                version: "1.0.0".into(),
            },
            &serde_json::to_vec(value).unwrap(),
        )
    }
    #[test]
    fn completion_verifies_the_exact_confirmed_signed_pack_before_install() {
        let root = std::env::temp_dir().join(format!("kit-download-signed-{}", std::process::id()));
        let stage = root.join("source");
        fs::create_dir_all(stage.join("files")).unwrap();
        let manifest = "id='ghost-world'\nname='Ghost World'\nversion='1.0.0'\nauthor='Nugwa'\ndescription='Test'\napi=1\nkind='full'";
        let mut pack = d2_modkit::MemoryPack::default();
        pack.files
            .insert("mod.toml".into(), manifest.as_bytes().to_vec());
        let key = d2_modkit::publish::author_key(&root.join("author.key")).unwrap();
        d2_modkit::publish::seal(&mut pack, &key).unwrap();
        for (name, bytes) in &pack.files {
            let path = stage.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }
        let path = root.join("ghost.d2pack");
        d2_native::pack::build(&stage, &path, 1).unwrap();
        let data = fs::read(&path).unwrap();
        let mut value = offer();
        value.bytes = data.len() as u64;
        value.sha256 = format!("{:x}", Sha256::digest(&data));
        value.author_key =
            d2_modkit::pack::hex(&d2_modkit::publish::verify(&pack).unwrap().signer.unwrap());
        prompt(&value)
            .unwrap()
            .confirm()
            .verify_package(&path)
            .unwrap();
        for field in 0..4 {
            let mut bad = value.clone();
            match field {
                0 => bad.bytes += 1,
                1 => bad.sha256 = "0".repeat(64),
                2 => bad.author_key = "0".repeat(64),
                _ => bad.author = "Someone else".into(),
            };
            assert!(prompt(&bad)
                .unwrap()
                .confirm()
                .verify_package(&path)
                .is_err());
        }
    }

    #[test]
    fn default_is_disabled_and_offers_require_exact_origin_identity_size_and_author() {
        assert_eq!(
            Settings::parse(DEFAULT_SETTINGS).unwrap(),
            Settings::default()
        );
        assert!(Prompt::from_official(
            &Settings::default(),
            "",
            &Required {
                id: "ghost-world".into(),
                version: "1.0.0".into()
            },
            &serde_json::to_vec(&offer()).unwrap()
        )
        .is_err());
        for text in [
            "official_list_url='http://example.test/mods'",
            "official_list_url='https://user@host/list'",
            "typo=''",
            "official_list_url=3",
        ] {
            assert!(Settings::parse(text).is_err(), "{text}");
        }
        let p = prompt(&offer()).unwrap();
        assert!(p.text().contains("Nugwa"));
        assert!(p.text().contains("123456 bytes"));
        for field in 0..6 {
            let mut bad = offer();
            match field {
                0 => bad.id = "another-mod".into(),
                1 => bad.version = "2.0.0".into(),
                2 => bad.bytes = 0,
                3 => bad.author = "bad\nauthor".into(),
                4 => bad.package_url = "https://unlisted.example/mod".into(),
                _ => bad.sha256 = "abc".into(),
            };
            assert!(prompt(&bad).is_err());
        }
        let root =
            std::env::temp_dir().join(format!("kit-download-consent-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let request = root.join(REQUEST_FILE);
        if request.exists() {
            fs::remove_file(&request).unwrap();
        }
        assert!(pending_under(&root).unwrap().is_none());
        assert!(!request.exists());
        p.confirm().queue(&root).unwrap();
        assert!(request.exists());
        assert!(prompt(&offer()).unwrap().confirm().queue(&root).is_err());
        fs::remove_file(request).unwrap();
    }
}
