use super::*;
use crate::storage::ObjectInfo;
use hpke::rand_core::SeedableRng;
use hpke::{
    Deserializable, Kem, OpModeR, OpModeS, Serializable, aead::ChaCha20Poly1305, kdf::HkdfSha256,
    kem::X25519HkdfSha256,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;
mod migration;
const MAGIC: &[u8] = b"WAYPOST1";
const INFO: &[u8] = b"waypost-storage-hpke-v1";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyConfig {
    active: String,
    keys: BTreeMap<String, String>,
}
pub(crate) struct Encryption {
    active: String,
    keys: BTreeMap<String, Zeroizing<[u8; 32]>>,
    allow_legacy: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    v: u8,
    kid: String,
    content_type: String,
}
impl Encryption {
    fn parse(input: &str, allow_legacy: bool) -> Result<Self> {
        let config: KeyConfig = crate::json::decode(crate::json::parse(input.as_bytes(), 8192)?)?;
        if config.keys.is_empty()
            || config.keys.len() > 4
            || !config.keys.contains_key(&config.active)
        {
            return Err(Error::invalid("Invalid encryption keyring."));
        }
        let mut keys = BTreeMap::new();
        for (id, secret) in config.keys {
            if id.len() > 32 {
                return Err(Error::invalid("Encryption key ID is too long."));
            }
            crate::domain::check_id(&id)?;
            let bytes = Zeroizing::new(hex::decode(secret).map_err(|_| {
                Error::invalid("Encryption keys must be 32-byte hexadecimal values.")
            })?);
            let key: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
                Error::invalid("Encryption keys must be 32-byte hexadecimal values.")
            })?;
            keys.insert(id, Zeroizing::new(key));
        }
        Ok(Self {
            active: config.active,
            keys,
            allow_legacy,
        })
    }
    fn keypair(
        &self,
        kid: &str,
        space: &str,
    ) -> Result<(
        <X25519HkdfSha256 as Kem>::PrivateKey,
        <X25519HkdfSha256 as Kem>::PublicKey,
    )> {
        let root = self
            .keys
            .get(kid)
            .ok_or_else(|| Error::internal("encryption key unavailable"))?;
        let mut mac = Hmac::<Sha256>::new_from_slice(root.as_ref())
            .map_err(|_| Error::internal("encryption key derivation"))?;
        mac.update(b"waypost-space-x25519-v1\0");
        mac.update(space.as_bytes());
        let seed = Zeroizing::new(mac.finalize().into_bytes().to_vec());
        Ok(X25519HkdfSha256::derive_keypair(&seed))
    }
    fn seal(&self, space: &str, key: &str, content_type: &str, bytes: &[u8]) -> Result<Vec<u8>> {
        let (_, public) = self.keypair(&self.active, space)?;
        let header = Header {
            v: 1,
            kid: self.active.clone(),
            content_type: content_type.into(),
        };
        let header_bytes =
            serde_json::to_vec(&header).map_err(|_| Error::internal("encryption header"))?;
        let aad =
            crate::json::canonical(&json!([space, key, URL_SAFE_NO_PAD.encode(&header_bytes)]))?;
        let mut rng = rand_chacha::ChaCha20Rng::from_seed(random_bytes::<32>()?);
        let (enc, cipher) = hpke::single_shot_seal_with_rng::<
            ChaCha20Poly1305,
            HkdfSha256,
            X25519HkdfSha256,
        >(&OpModeS::Base, &public, INFO, bytes, &aad, &mut rng)
        .map_err(|_| Error::internal("HPKE encryption"))?;
        let mut result = MAGIC.to_vec();
        result.extend((header_bytes.len() as u32).to_be_bytes());
        result.extend(header_bytes);
        result.extend(enc.to_bytes());
        result.extend(cipher);
        Ok(result)
    }
    fn open(&self, space: &str, key: &str, bytes: &[u8]) -> Result<(Vec<u8>, String)> {
        if bytes.len() < 12 + 32 + 16 || !bytes.starts_with(MAGIC) {
            return Err(Error::internal("encrypted object format"));
        }
        let len = u32::from_be_bytes(
            bytes[8..12]
                .try_into()
                .map_err(|_| Error::internal("encrypted header size"))?,
        ) as usize;
        if len > 2048 || bytes.len() < 12 + len + 32 + 16 {
            return Err(Error::internal("encrypted header size"));
        }
        let raw = &bytes[12..12 + len];
        let header: Header =
            serde_json::from_slice(raw).map_err(|_| Error::internal("encrypted header"))?;
        if header.v != 1 {
            return Err(Error::internal("encryption version"));
        }
        let (private, _) = self.keypair(&header.kid, space)?;
        let enc =
            <X25519HkdfSha256 as Kem>::EncappedKey::from_bytes(&bytes[12 + len..12 + len + 32])
                .map_err(|_| Error::internal("encryption encapsulation"))?;
        let aad = crate::json::canonical(&json!([space, key, URL_SAFE_NO_PAD.encode(raw)]))?;
        let plain = hpke::single_shot_open::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256>(
            &OpModeR::Base,
            &private,
            &enc,
            INFO,
            &bytes[12 + len + 32..],
            &aad,
        )
        .map_err(|_| Error::internal("encrypted content authentication"))?;
        Ok((plain, header.content_type))
    }
}
impl<D: Database, O: ObjectStore> App<D, O> {
    pub(super) fn storage_key(&self, space: &str, logical: &str) -> String {
        if self.encryption.is_none() {
            return logical.into();
        }
        format!(
            "object_{}_{}",
            hex::encode(&Sha256::digest(space.as_bytes())[..16]),
            hex::encode(&Sha256::digest(logical.as_bytes())[..16])
        )
    }
    pub fn with_encryption_keys(mut self, keyring: &str, allow_legacy: bool) -> Result<Self> {
        self.encryption = Some(Encryption::parse(keyring, allow_legacy)?);
        Ok(self)
    }
    pub(super) fn seal_field(&self, space: &str, context: &str, plain: &str) -> Result<String> {
        match &self.encryption {
            Some(e) => Ok(format!(
                "wpenc1:{}",
                URL_SAFE_NO_PAD.encode(e.seal(space, context, "text/plain", plain.as_bytes())?)
            )),
            None => Ok(plain.into()),
        }
    }
    pub(super) fn open_field(&self, space: &str, context: &str, stored: &str) -> Result<String> {
        if let Some(encoded) = stored.strip_prefix("wpenc1:") {
            let e = self
                .encryption
                .as_ref()
                .ok_or_else(|| Error::internal("encryption configuration missing"))?;
            let bytes = URL_SAFE_NO_PAD
                .decode(encoded)
                .map_err(|_| Error::internal("encrypted field encoding"))?;
            let (plain, _) = e.open(space, context, &bytes)?;
            return String::from_utf8(plain)
                .map_err(|_| Error::internal("encrypted text encoding"));
        }
        if self.encryption.as_ref().is_some_and(|e| !e.allow_legacy) {
            return Err(Error::internal("plaintext data requires migration"));
        }
        Ok(stored.into())
    }
    pub(super) async fn object_read(&self, space: &str, key: &str) -> Result<Vec<u8>> {
        let bytes = self.objects.read(key).await?;
        if bytes.starts_with(MAGIC) {
            return Ok(self
                .encryption
                .as_ref()
                .ok_or_else(|| Error::internal("encryption configuration missing"))?
                .open(space, key, &bytes)?
                .0);
        }
        if self.encryption.as_ref().is_some_and(|e| !e.allow_legacy) {
            return Err(Error::internal("plaintext object requires migration"));
        }
        Ok(bytes)
    }
    pub(super) async fn object_inspect(
        &self,
        space: &str,
        key: &str,
    ) -> Result<Option<ObjectInfo>> {
        let Some(info) = self.objects.inspect(key).await? else {
            return Ok(None);
        };
        if self.encryption.is_none() {
            return Ok(Some(info));
        }
        let bytes = self.objects.read(key).await?;
        let (plain, content_type) = if bytes.starts_with(MAGIC) {
            self.encryption
                .as_ref()
                .ok_or_else(|| Error::internal("encryption configuration"))?
                .open(space, key, &bytes)?
        } else if self.encryption.as_ref().is_some_and(|e| e.allow_legacy) {
            (bytes, info.content_type)
        } else {
            return Err(Error::internal("plaintext object requires migration"));
        };
        Ok(Some(ObjectInfo {
            size: plain.len() as u64,
            sha256: Sha256::digest(&plain).to_vec(),
            content_type,
        }))
    }
    pub(super) async fn object_create(
        &self,
        space: &str,
        key: &str,
        bytes: &[u8],
        content_type: &str,
        sha: &[u8],
    ) -> Result<ObjectInfo> {
        if Sha256::digest(bytes).as_slice() != sha {
            return Err(Error::new(
                "CHECKSUM_MISMATCH",
                "Content checksum did not match.",
            ));
        }
        let Some(encryption) = &self.encryption else {
            return self
                .objects
                .create_only(key, bytes, content_type, sha)
                .await;
        };
        let matches = |info: ObjectInfo| -> Result<ObjectInfo> {
            if info.size != bytes.len() as u64
                || info.sha256 != sha
                || info.content_type != content_type
            {
                return Err(Error::new(
                    "UPLOAD_CONTENT_CONFLICT",
                    "Object content is immutable.",
                ));
            }
            Ok(info)
        };
        if let Some(info) = self.object_inspect(space, key).await? {
            return matches(info);
        }
        let cipher = encryption.seal(space, key, content_type, bytes)?;
        match self
            .objects
            .create_only(
                key,
                &cipher,
                "application/vnd.waypost.encrypted",
                &Sha256::digest(&cipher),
            )
            .await
        {
            Ok(_) => {}
            Err(e) if e.code == "UPLOAD_CONTENT_CONFLICT" => {}
            Err(e) => return Err(e),
        }
        matches(
            self.object_inspect(space, key)
                .await?
                .ok_or_else(Error::storage)?,
        )
    }
}
