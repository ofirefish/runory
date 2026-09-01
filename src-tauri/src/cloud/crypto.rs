use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use getrandom::rand_core::{SeedableRng, TryRng};
use getrandom::SysRng;
use hpke::aead::ChaCha20Poly1305;
use hpke::kdf::HkdfSha256;
use hpke::kem::{Kem as KemTrait, X25519HkdfSha256};
use hpke::{
    single_shot_open, single_shot_seal_with_rng, Deserializable, OpModeR, OpModeS, Serializable,
};
use rand_chacha::ChaCha20Rng;
use ring::{digest, hmac};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::domain::{AppError, AppResult};

const OBJECT_FORMAT_VERSION: u8 = 3;
const OBJECT_CIPHERSUITE: u16 = 1;
const HPKE_ENVELOPE_VERSION: u8 = 1;
const HPKE_CIPHERSUITE: u16 = 1;
const AEAD_NONCE_LENGTH: usize = 12;
const AEAD_TAG_LENGTH: usize = 16;
const KEY_LENGTH: usize = 32;
const SIGNATURE_LENGTH: usize = 64;
const VAULT_KEYSET_LENGTH: usize = 1 + 16 + 8 + KEY_LENGTH + KEY_LENGTH + KEY_LENGTH;
const MAX_OBJECT_PLAINTEXT_BYTES: usize = 2 * 1024 * 1024;
const HPKE_INFO: &[u8] = b"runory-vault-keyset-hpke-v1";
const HPKE_AAD_DOMAIN: &[u8] = b"runory-vault-keyset-aad-v1";
const LOOKUP_DOMAIN: &[u8] = b"runory-cloud-lookup-v3";
const HEADER_DOMAIN: &[u8] = b"runory-cloud-object-header-v3";
const WRAP_AAD_DOMAIN: &[u8] = b"runory-cloud-object-wrap-v3";
const PAYLOAD_AAD_DOMAIN: &[u8] = b"runory-cloud-object-payload-v3";
const SIGNATURE_DOMAIN: &[u8] = b"runory-cloud-object-signature-v3";

type HpkeKem = X25519HkdfSha256;
type HpkeKdf = HkdfSha256;
type HpkeAead = ChaCha20Poly1305;

pub(crate) struct DeviceSigningKey {
    inner: SigningKey,
}

impl DeviceSigningKey {
    pub(crate) fn generate() -> AppResult<Self> {
        let mut seed = Zeroizing::new(random_array::<KEY_LENGTH>()?);
        let inner = SigningKey::from_bytes(&seed);
        seed.zeroize();
        Ok(Self { inner })
    }

    pub(crate) fn from_seed(mut seed: Zeroizing<[u8; KEY_LENGTH]>) -> Self {
        let inner = SigningKey::from_bytes(&seed);
        seed.zeroize();
        Self { inner }
    }

    pub(crate) fn to_seed(&self) -> Zeroizing<[u8; KEY_LENGTH]> {
        Zeroizing::new(self.inner.to_bytes())
    }

    pub(crate) fn public_key(&self) -> [u8; KEY_LENGTH] {
        self.inner.verifying_key().to_bytes()
    }

    fn sign(&self, message: &[u8]) -> [u8; SIGNATURE_LENGTH] {
        self.inner.sign(message).to_bytes()
    }
}

pub(crate) struct DeviceEncryptionKeyPair {
    private_key: Zeroizing<[u8; KEY_LENGTH]>,
    public_key: [u8; KEY_LENGTH],
}

impl DeviceEncryptionKeyPair {
    pub(crate) fn generate() -> AppResult<Self> {
        let mut ikm = Zeroizing::new(random_array::<KEY_LENGTH>()?);
        let (private_key, public_key) = HpkeKem::derive_keypair(ikm.as_ref());
        ikm.zeroize();
        let serialized_private = private_key.to_bytes();
        let serialized_public = public_key.to_bytes();
        let private_key = Zeroizing::new(
            serialized_private
                .as_slice()
                .try_into()
                .map_err(|_| AppError::CloudCrypto)?,
        );
        let public_key = serialized_public
            .as_slice()
            .try_into()
            .map_err(|_| AppError::CloudCrypto)?;
        Ok(Self {
            private_key,
            public_key,
        })
    }

    pub(crate) fn from_private_bytes(private_key: Zeroizing<[u8; KEY_LENGTH]>) -> AppResult<Self> {
        let parsed = <HpkeKem as KemTrait>::PrivateKey::from_bytes(private_key.as_ref())
            .map_err(|_| AppError::CloudInvalid)?;
        let public_key = HpkeKem::sk_to_pk(&parsed).to_bytes();
        Ok(Self {
            private_key,
            public_key: public_key
                .as_slice()
                .try_into()
                .map_err(|_| AppError::CloudCrypto)?,
        })
    }

    pub(crate) fn private_bytes(&self) -> Zeroizing<[u8; KEY_LENGTH]> {
        Zeroizing::new(*self.private_key)
    }

    pub(crate) fn public_key(&self) -> [u8; KEY_LENGTH] {
        self.public_key
    }
}

pub(crate) struct VaultKeyset {
    vault_id: Uuid,
    epoch: u64,
    index_key: Zeroizing<[u8; KEY_LENGTH]>,
    epoch_key: Zeroizing<[u8; KEY_LENGTH]>,
    checkpoint_digest: [u8; KEY_LENGTH],
}

impl VaultKeyset {
    pub(crate) fn generate(vault_id: Uuid, epoch: u64) -> AppResult<Self> {
        if vault_id.is_nil() || epoch == 0 {
            return Err(AppError::CloudInvalid);
        }
        Ok(Self {
            vault_id,
            epoch,
            index_key: Zeroizing::new(random_array::<KEY_LENGTH>()?),
            epoch_key: Zeroizing::new(random_array::<KEY_LENGTH>()?),
            checkpoint_digest: [0; KEY_LENGTH],
        })
    }

    pub(crate) fn vault_id(&self) -> Uuid {
        self.vault_id
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(crate) fn index_key(&self) -> &[u8; KEY_LENGTH] {
        &self.index_key
    }

    pub(crate) fn epoch_key(&self) -> &[u8; KEY_LENGTH] {
        &self.epoch_key
    }

    pub(crate) fn checkpoint_digest(&self) -> &[u8; KEY_LENGTH] {
        &self.checkpoint_digest
    }

    fn encode(&self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(VAULT_KEYSET_LENGTH));
        bytes.push(HPKE_ENVELOPE_VERSION);
        bytes.extend_from_slice(self.vault_id.as_bytes());
        bytes.extend_from_slice(&self.epoch.to_be_bytes());
        bytes.extend_from_slice(self.index_key.as_ref());
        bytes.extend_from_slice(self.epoch_key.as_ref());
        bytes.extend_from_slice(&self.checkpoint_digest);
        bytes
    }

    fn decode(bytes: &[u8]) -> AppResult<Self> {
        if bytes.len() != VAULT_KEYSET_LENGTH || bytes[0] != HPKE_ENVELOPE_VERSION {
            return Err(AppError::CloudInvalid);
        }
        let vault_id = Uuid::from_slice(&bytes[1..17]).map_err(|_| AppError::CloudInvalid)?;
        let epoch = u64::from_be_bytes(
            bytes[17..25]
                .try_into()
                .map_err(|_| AppError::CloudInvalid)?,
        );
        if vault_id.is_nil() || epoch == 0 {
            return Err(AppError::CloudInvalid);
        }
        Ok(Self {
            vault_id,
            epoch,
            index_key: Zeroizing::new(
                bytes[25..57]
                    .try_into()
                    .map_err(|_| AppError::CloudInvalid)?,
            ),
            epoch_key: Zeroizing::new(
                bytes[57..89]
                    .try_into()
                    .map_err(|_| AppError::CloudInvalid)?,
            ),
            checkpoint_digest: bytes[89..121]
                .try_into()
                .map_err(|_| AppError::CloudInvalid)?,
        })
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HpkeEnvelope {
    pub version: u8,
    pub ciphersuite: u16,
    pub encapsulated_key: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

pub(crate) fn seal_vault_keyset(
    recipient_device_id: Uuid,
    recipient_public_key: &[u8; KEY_LENGTH],
    keyset: &VaultKeyset,
) -> AppResult<HpkeEnvelope> {
    if recipient_device_id.is_nil() {
        return Err(AppError::CloudInvalid);
    }
    let public_key = <HpkeKem as KemTrait>::PublicKey::from_bytes(recipient_public_key)
        .map_err(|_| AppError::CloudInvalid)?;
    let plaintext = keyset.encode();
    let aad = hpke_aad(keyset.vault_id, recipient_device_id);
    let mut rng = fallible_csprng()?;
    let (encapsulated_key, ciphertext) = single_shot_seal_with_rng::<HpkeAead, HpkeKdf, HpkeKem>(
        &OpModeS::Base,
        &public_key,
        HPKE_INFO,
        plaintext.as_slice(),
        &aad,
        &mut rng,
    )
    .map_err(|_| AppError::CloudCrypto)?;
    Ok(HpkeEnvelope {
        version: HPKE_ENVELOPE_VERSION,
        ciphersuite: HPKE_CIPHERSUITE,
        encapsulated_key: encapsulated_key.to_bytes().as_slice().to_vec(),
        ciphertext,
    })
}

pub(crate) fn open_vault_keyset(
    expected_vault_id: Uuid,
    recipient_device_id: Uuid,
    recipient_private_key: &[u8; KEY_LENGTH],
    envelope: &HpkeEnvelope,
) -> AppResult<VaultKeyset> {
    if expected_vault_id.is_nil()
        || recipient_device_id.is_nil()
        || envelope.version != HPKE_ENVELOPE_VERSION
        || envelope.ciphersuite != HPKE_CIPHERSUITE
        || envelope.encapsulated_key.len() != KEY_LENGTH
        || envelope.ciphertext.len() != VAULT_KEYSET_LENGTH + AEAD_TAG_LENGTH
    {
        return Err(AppError::CloudInvalid);
    }
    let private_key = <HpkeKem as KemTrait>::PrivateKey::from_bytes(recipient_private_key)
        .map_err(|_| AppError::CloudInvalid)?;
    let encapsulated_key =
        <HpkeKem as KemTrait>::EncappedKey::from_bytes(&envelope.encapsulated_key)
            .map_err(|_| AppError::CloudInvalid)?;
    let aad = hpke_aad(expected_vault_id, recipient_device_id);
    let plaintext = Zeroizing::new(
        single_shot_open::<HpkeAead, HpkeKdf, HpkeKem>(
            &OpModeR::Base,
            &private_key,
            &encapsulated_key,
            HPKE_INFO,
            &envelope.ciphertext,
            &aad,
        )
        .map_err(|_| AppError::CloudDecrypt)?,
    );
    let keyset = VaultKeyset::decode(plaintext.as_slice())?;
    if keyset.vault_id != expected_vault_id {
        return Err(AppError::CloudDecrypt);
    }
    Ok(keyset)
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CloudObjectHeaderV3 {
    pub version: u8,
    pub ciphersuite: u16,
    pub vault_id: Uuid,
    pub opaque_lookup_id: [u8; KEY_LENGTH],
    pub key_epoch: u64,
    pub content_revision: u64,
    pub wrap_revision: u64,
    pub parent_digest: [u8; KEY_LENGTH],
    pub author_device_id: Uuid,
    pub wrap_nonce: [u8; AEAD_NONCE_LENGTH],
    pub payload_nonce: [u8; AEAD_NONCE_LENGTH],
    pub plaintext_length: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CloudObjectEnvelopeV3 {
    pub header: CloudObjectHeaderV3,
    pub wrapped_object_key: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub digest: [u8; KEY_LENGTH],
    pub signature: Vec<u8>,
}

pub(crate) struct NewObjectContext {
    pub vault_id: Uuid,
    pub opaque_lookup_id: [u8; KEY_LENGTH],
    pub key_epoch: u64,
    pub content_revision: u64,
    pub wrap_revision: u64,
    pub parent_digest: [u8; KEY_LENGTH],
    pub author_device_id: Uuid,
}

pub(crate) fn derive_lookup_id(
    index_key: &[u8; KEY_LENGTH],
    object_kind: &[u8],
    logical_id: Uuid,
) -> AppResult<[u8; KEY_LENGTH]> {
    if object_kind.is_empty() || object_kind.len() > 32 || logical_id.is_nil() {
        return Err(AppError::CloudInvalid);
    }
    let key = hmac::Key::new(hmac::HMAC_SHA256, index_key);
    let mut context = hmac::Context::with_key(&key);
    context.update(LOOKUP_DOMAIN);
    context.update(&[object_kind.len() as u8]);
    context.update(object_kind);
    context.update(logical_id.as_bytes());
    context
        .sign()
        .as_ref()
        .try_into()
        .map_err(|_| AppError::CloudCrypto)
}

pub(crate) fn seal_object(
    context: NewObjectContext,
    vault_epoch_key: &[u8; KEY_LENGTH],
    plaintext: &[u8],
    signing_key: &DeviceSigningKey,
) -> AppResult<CloudObjectEnvelopeV3> {
    validate_new_object_context(&context, plaintext.len())?;
    let object_key = Zeroizing::new(random_array::<KEY_LENGTH>()?);
    let wrap_nonce = random_array::<AEAD_NONCE_LENGTH>()?;
    let payload_nonce = random_array::<AEAD_NONCE_LENGTH>()?;
    let header = CloudObjectHeaderV3 {
        version: OBJECT_FORMAT_VERSION,
        ciphersuite: OBJECT_CIPHERSUITE,
        vault_id: context.vault_id,
        opaque_lookup_id: context.opaque_lookup_id,
        key_epoch: context.key_epoch,
        content_revision: context.content_revision,
        wrap_revision: context.wrap_revision,
        parent_digest: context.parent_digest,
        author_device_id: context.author_device_id,
        wrap_nonce,
        payload_nonce,
        plaintext_length: plaintext
            .len()
            .try_into()
            .map_err(|_| AppError::CloudInvalid)?,
    };
    let encoded_header = encode_header(&header);
    let wrapped_object_key = encrypt_aead(
        vault_epoch_key,
        &header.wrap_nonce,
        object_key.as_ref(),
        &domain_aad(WRAP_AAD_DOMAIN, &encoded_header),
    )?;
    let ciphertext = encrypt_aead(
        object_key.as_ref(),
        &header.payload_nonce,
        plaintext,
        &domain_aad(PAYLOAD_AAD_DOMAIN, &encoded_header),
    )?;
    let object_digest = object_digest(&encoded_header, &wrapped_object_key, &ciphertext)?;
    let signature = signing_key.sign(&object_digest).to_vec();
    Ok(CloudObjectEnvelopeV3 {
        header,
        wrapped_object_key,
        ciphertext,
        digest: object_digest,
        signature,
    })
}

pub(crate) fn open_object(
    expected_vault_id: Uuid,
    expected_lookup_id: &[u8; KEY_LENGTH],
    vault_epoch_key: &[u8; KEY_LENGTH],
    author_public_key: &[u8; KEY_LENGTH],
    envelope: &CloudObjectEnvelopeV3,
) -> AppResult<Zeroizing<Vec<u8>>> {
    validate_envelope(expected_vault_id, expected_lookup_id, envelope)?;
    let encoded_header = encode_header(&envelope.header);
    let expected_digest = object_digest(
        &encoded_header,
        &envelope.wrapped_object_key,
        &envelope.ciphertext,
    )?;
    if expected_digest != envelope.digest {
        return Err(AppError::CloudDecrypt);
    }
    verify_signature(author_public_key, &envelope.digest, &envelope.signature)?;
    let object_key = Zeroizing::new(decrypt_aead(
        vault_epoch_key,
        &envelope.header.wrap_nonce,
        &envelope.wrapped_object_key,
        &domain_aad(WRAP_AAD_DOMAIN, &encoded_header),
    )?);
    if object_key.len() != KEY_LENGTH {
        return Err(AppError::CloudDecrypt);
    }
    let plaintext = Zeroizing::new(decrypt_aead(
        object_key.as_slice(),
        &envelope.header.payload_nonce,
        &envelope.ciphertext,
        &domain_aad(PAYLOAD_AAD_DOMAIN, &encoded_header),
    )?);
    if plaintext.len() != envelope.header.plaintext_length as usize {
        return Err(AppError::CloudDecrypt);
    }
    Ok(plaintext)
}

fn hpke_aad(vault_id: Uuid, recipient_device_id: Uuid) -> Vec<u8> {
    let mut aad = Vec::with_capacity(HPKE_AAD_DOMAIN.len() + 32);
    aad.extend_from_slice(HPKE_AAD_DOMAIN);
    aad.extend_from_slice(vault_id.as_bytes());
    aad.extend_from_slice(recipient_device_id.as_bytes());
    aad
}

fn validate_new_object_context(context: &NewObjectContext, plaintext_len: usize) -> AppResult<()> {
    if context.vault_id.is_nil()
        || context.author_device_id.is_nil()
        || context.opaque_lookup_id == [0; KEY_LENGTH]
        || context.key_epoch == 0
        || context.content_revision == 0
        || context.wrap_revision == 0
        || plaintext_len == 0
        || plaintext_len > MAX_OBJECT_PLAINTEXT_BYTES
    {
        Err(AppError::CloudInvalid)
    } else {
        Ok(())
    }
}

fn validate_envelope(
    expected_vault_id: Uuid,
    expected_lookup_id: &[u8; KEY_LENGTH],
    envelope: &CloudObjectEnvelopeV3,
) -> AppResult<()> {
    let header = &envelope.header;
    if expected_vault_id.is_nil()
        || header.version != OBJECT_FORMAT_VERSION
        || header.ciphersuite != OBJECT_CIPHERSUITE
        || header.vault_id != expected_vault_id
        || &header.opaque_lookup_id != expected_lookup_id
        || header.opaque_lookup_id == [0; KEY_LENGTH]
        || header.key_epoch == 0
        || header.content_revision == 0
        || header.wrap_revision == 0
        || header.author_device_id.is_nil()
        || header.plaintext_length == 0
        || header.plaintext_length as usize > MAX_OBJECT_PLAINTEXT_BYTES
        || envelope.wrapped_object_key.len() != KEY_LENGTH + AEAD_TAG_LENGTH
        || envelope.ciphertext.len() != header.plaintext_length as usize + AEAD_TAG_LENGTH
        || envelope.signature.len() != SIGNATURE_LENGTH
    {
        Err(AppError::CloudInvalid)
    } else {
        Ok(())
    }
}

fn verify_signature(
    public_key: &[u8; KEY_LENGTH],
    message: &[u8],
    signature: &[u8],
) -> AppResult<()> {
    let verifying_key = VerifyingKey::from_bytes(public_key).map_err(|_| AppError::CloudInvalid)?;
    let signature = Signature::from_slice(signature).map_err(|_| AppError::CloudInvalid)?;
    verifying_key
        .verify(message, &signature)
        .map_err(|_| AppError::CloudDecrypt)
}

fn encode_header(header: &CloudObjectHeaderV3) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(181);
    bytes.extend_from_slice(HEADER_DOMAIN);
    bytes.push(header.version);
    bytes.extend_from_slice(&header.ciphersuite.to_be_bytes());
    bytes.extend_from_slice(header.vault_id.as_bytes());
    bytes.extend_from_slice(&header.opaque_lookup_id);
    bytes.extend_from_slice(&header.key_epoch.to_be_bytes());
    bytes.extend_from_slice(&header.content_revision.to_be_bytes());
    bytes.extend_from_slice(&header.wrap_revision.to_be_bytes());
    bytes.extend_from_slice(&header.parent_digest);
    bytes.extend_from_slice(header.author_device_id.as_bytes());
    bytes.extend_from_slice(&header.wrap_nonce);
    bytes.extend_from_slice(&header.payload_nonce);
    bytes.extend_from_slice(&header.plaintext_length.to_be_bytes());
    bytes
}

fn domain_aad(domain: &[u8], encoded_header: &[u8]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(domain.len() + encoded_header.len());
    aad.extend_from_slice(domain);
    aad.extend_from_slice(encoded_header);
    aad
}

fn object_digest(
    encoded_header: &[u8],
    wrapped_object_key: &[u8],
    ciphertext: &[u8],
) -> AppResult<[u8; KEY_LENGTH]> {
    let wrapped_len: u32 = wrapped_object_key
        .len()
        .try_into()
        .map_err(|_| AppError::CloudInvalid)?;
    let ciphertext_len: u32 = ciphertext
        .len()
        .try_into()
        .map_err(|_| AppError::CloudInvalid)?;
    let mut context = digest::Context::new(&digest::SHA256);
    context.update(SIGNATURE_DOMAIN);
    context.update(encoded_header);
    context.update(&wrapped_len.to_be_bytes());
    context.update(wrapped_object_key);
    context.update(&ciphertext_len.to_be_bytes());
    context.update(ciphertext);
    context
        .finish()
        .as_ref()
        .try_into()
        .map_err(|_| AppError::CloudCrypto)
}

fn encrypt_aead(
    key: &[u8],
    nonce: &[u8; AEAD_NONCE_LENGTH],
    plaintext: &[u8],
    aad: &[u8],
) -> AppResult<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| AppError::CloudCrypto)?;
    cipher
        .encrypt(
            &Nonce::from(*nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| AppError::CloudCrypto)
}

fn decrypt_aead(
    key: &[u8],
    nonce: &[u8; AEAD_NONCE_LENGTH],
    ciphertext: &[u8],
    aad: &[u8],
) -> AppResult<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| AppError::CloudDecrypt)?;
    cipher
        .decrypt(
            &Nonce::from(*nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| AppError::CloudDecrypt)
}

fn random_array<const N: usize>() -> AppResult<[u8; N]> {
    let mut bytes = [0; N];
    SysRng
        .try_fill_bytes(&mut bytes)
        .map_err(|_| AppError::CloudCrypto)?;
    Ok(bytes)
}

fn fallible_csprng() -> AppResult<ChaCha20Rng> {
    let mut seed = Zeroizing::new(random_array::<KEY_LENGTH>()?);
    let rng = ChaCha20Rng::from_seed(*seed);
    seed.zeroize();
    Ok(rng)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (DeviceSigningKey, DeviceEncryptionKeyPair, VaultKeyset, Uuid) {
        let signing = DeviceSigningKey::from_seed(Zeroizing::new([7; KEY_LENGTH]));
        let encryption =
            DeviceEncryptionKeyPair::from_private_bytes(Zeroizing::new([11; KEY_LENGTH]))
                .expect("fixed X25519 private key should parse");
        let device_id = Uuid::from_u128(0x11111111111111111111111111111111);
        let keyset = VaultKeyset {
            vault_id: Uuid::from_u128(0x22222222222222222222222222222222),
            epoch: 3,
            index_key: Zeroizing::new([13; KEY_LENGTH]),
            epoch_key: Zeroizing::new([17; KEY_LENGTH]),
            checkpoint_digest: [19; KEY_LENGTH],
        };
        (signing, encryption, keyset, device_id)
    }

    fn object_context(keyset: &VaultKeyset, device_id: Uuid) -> NewObjectContext {
        NewObjectContext {
            vault_id: keyset.vault_id(),
            opaque_lookup_id: derive_lookup_id(
                keyset.index_key(),
                b"profile",
                Uuid::from_u128(0x33333333333333333333333333333333),
            )
            .expect("lookup id should derive"),
            key_epoch: keyset.epoch(),
            content_revision: 1,
            wrap_revision: 1,
            parent_digest: [0; KEY_LENGTH],
            author_device_id: device_id,
        }
    }

    #[test]
    fn vault_keyset_hpke_round_trip_is_context_bound() {
        let (_, encryption, keyset, device_id) = fixture();
        let envelope = seal_vault_keyset(device_id, &encryption.public_key(), &keyset)
            .expect("keyset should seal");
        let recipient_private_key = encryption.private_bytes();
        let opened = open_vault_keyset(
            keyset.vault_id(),
            device_id,
            &recipient_private_key,
            &envelope,
        )
        .expect("keyset should open");
        assert_eq!(opened.vault_id(), keyset.vault_id());
        assert_eq!(opened.epoch(), keyset.epoch());
        assert_eq!(opened.index_key(), keyset.index_key());
        assert_eq!(opened.epoch_key(), keyset.epoch_key());
        assert_eq!(opened.checkpoint_digest(), keyset.checkpoint_digest());

        let wrong_device = Uuid::from_u128(0x44444444444444444444444444444444);
        assert!(matches!(
            open_vault_keyset(
                keyset.vault_id(),
                wrong_device,
                &recipient_private_key,
                &envelope,
            ),
            Err(AppError::CloudDecrypt)
        ));
    }

    #[test]
    fn object_round_trip_is_signed_and_uses_fresh_keys() {
        let (signing, _, keyset, device_id) = fixture();
        let plaintext = br#"{"type":"profile","id":"example"}"#;
        let first = seal_object(
            object_context(&keyset, device_id),
            keyset.epoch_key(),
            plaintext,
            &signing,
        )
        .expect("first object should seal");
        let second = seal_object(
            object_context(&keyset, device_id),
            keyset.epoch_key(),
            plaintext,
            &signing,
        )
        .expect("second object should seal");
        assert_ne!(first.header.payload_nonce, second.header.payload_nonce);
        assert_ne!(first.wrapped_object_key, second.wrapped_object_key);
        assert_ne!(first.ciphertext, second.ciphertext);

        let opened = open_object(
            keyset.vault_id(),
            &first.header.opaque_lookup_id,
            keyset.epoch_key(),
            &signing.public_key(),
            &first,
        )
        .expect("object should open");
        assert_eq!(opened.as_slice(), plaintext);
    }

    #[test]
    fn object_tampering_and_cross_vault_replay_fail_closed() {
        let (signing, _, keyset, device_id) = fixture();
        let object = seal_object(
            object_context(&keyset, device_id),
            keyset.epoch_key(),
            b"secret",
            &signing,
        )
        .expect("object should seal");

        let mut tampered = object.clone();
        tampered.ciphertext[0] ^= 1;
        assert!(matches!(
            open_object(
                keyset.vault_id(),
                &tampered.header.opaque_lookup_id,
                keyset.epoch_key(),
                &signing.public_key(),
                &tampered,
            ),
            Err(AppError::CloudDecrypt)
        ));

        let other_vault = Uuid::from_u128(0x55555555555555555555555555555555);
        assert!(matches!(
            open_object(
                other_vault,
                &object.header.opaque_lookup_id,
                keyset.epoch_key(),
                &signing.public_key(),
                &object,
            ),
            Err(AppError::CloudInvalid)
        ));
    }

    #[test]
    fn canonical_header_changes_when_security_context_changes() {
        let (_, _, keyset, device_id) = fixture();
        let context = object_context(&keyset, device_id);
        let header = CloudObjectHeaderV3 {
            version: OBJECT_FORMAT_VERSION,
            ciphersuite: OBJECT_CIPHERSUITE,
            vault_id: context.vault_id,
            opaque_lookup_id: context.opaque_lookup_id,
            key_epoch: context.key_epoch,
            content_revision: context.content_revision,
            wrap_revision: context.wrap_revision,
            parent_digest: context.parent_digest,
            author_device_id: context.author_device_id,
            wrap_nonce: [23; AEAD_NONCE_LENGTH],
            payload_nonce: [29; AEAD_NONCE_LENGTH],
            plaintext_length: 42,
        };
        let encoded = encode_header(&header);
        let mut changed = header.clone();
        changed.content_revision += 1;
        assert_ne!(encoded, encode_header(&changed));
        assert_eq!(encoded.len(), HEADER_DOMAIN.len() + 151);
    }

    #[test]
    fn rfc_9180_x25519_chacha_known_answer_opens() {
        let sk = <HpkeKem as KemTrait>::PrivateKey::from_bytes(&hex(
            "8057991eef8f1f1af18f4a9491d16a1ce333f695d4db8e38da75975c4478e0fb",
        ))
        .expect("RFC private key should parse");
        let enc = <HpkeKem as KemTrait>::EncappedKey::from_bytes(&hex(
            "1afa08d3dec047a643885163f1180476fa7ddb54c6a8029ea33f95796bf2ac4a",
        ))
        .expect("RFC encapsulated key should parse");
        let plaintext = single_shot_open::<HpkeAead, HpkeKdf, HpkeKem>(
            &OpModeR::Base,
            &sk,
            &enc,
            &hex("4f6465206f6e2061204772656369616e2055726e"),
            &hex("1c5250d8034ec2b784ba2cfd69dbdb8af406cfe3ff938e131f0def8c8b60b4db21993c62ce81883d2dd1b51a28"),
            &hex("436f756e742d30"),
        )
        .expect("RFC 9180 vector should open");
        assert_eq!(
            plaintext,
            hex("4265617574792069732074727574682c20747275746820626561757479")
        );
    }

    #[test]
    fn nist_aes_256_gcm_known_answer_matches() {
        let key: [u8; KEY_LENGTH] =
            hex("0000000000000000000000000000000000000000000000000000000000000000")
                .try_into()
                .expect("NIST AES-256 key has the expected length");
        let nonce: [u8; AEAD_NONCE_LENGTH] = hex("000000000000000000000000")
            .try_into()
            .expect("NIST nonce has the expected length");
        let ciphertext = encrypt_aead(&key, &nonce, &hex("00000000000000000000000000000000"), &[])
            .expect("NIST plaintext should encrypt");
        assert_eq!(
            ciphertext,
            hex("cea7403d4d606b6e074ec5d3baf39d18d0d1c8a799996bf0265b98b5d48ab919")
        );
    }

    #[test]
    fn rfc_8032_ed25519_known_answer_matches() {
        let seed: [u8; KEY_LENGTH] =
            hex("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60")
                .try_into()
                .expect("RFC seed has the expected length");
        let signing = DeviceSigningKey::from_seed(Zeroizing::new(seed));
        assert_eq!(
            signing.public_key().as_slice(),
            hex("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
        );
        assert_eq!(
            signing.sign(&[]).as_slice(),
            hex("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b")
        );
    }

    fn hex(value: &str) -> Vec<u8> {
        assert_eq!(value.len() % 2, 0);
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let text = std::str::from_utf8(pair).expect("test vector is ASCII");
                u8::from_str_radix(text, 16).expect("test vector is hex")
            })
            .collect()
    }
}
