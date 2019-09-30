use secstr::SecUtf8;
use zeroize::Zeroize;

use client_common::{PrivateKey, PublicKey, Result, SecureStorage, Storage};

const KEYSPACE: &str = "core_key";

use super::hdkey_service::HDKeyService;
use super::key_service_data::KeyServiceInterface;
use super::key_service_data::WalletKinds;
use super::simple_key_service::SimpleKeyService;

/// wallet facade
/// Maintains mapping `public-key -> private-key`
#[derive(Debug, Default, Clone)]
pub struct KeyService<T: Storage> {
    storage: T,
    kind: WalletKinds,
}

impl<T> KeyService<T>
where
    T: Storage,
{
    /// Creates a new instance of key service
    pub fn new(storage: T, kind: WalletKinds) -> Self {
        KeyService { storage, kind }
    }

    /// Generates a new public-private keypair
    pub fn generate_keypair(
        &self,
        _name: &str,
        passphrase: &SecUtf8,
        _is_staking: bool,
    ) -> Result<(PublicKey, PrivateKey)> {
        let private_key = PrivateKey::new()?;
        let public_key = PublicKey::from(&private_key);

        self.storage.set_secure(
            KEYSPACE,
            public_key.serialize(),
            private_key.serialize(),
            passphrase,
        )?;

        Ok((public_key, private_key))
    }

    /// Retrieves private key corresponding to given public key
    pub fn private_key(
        &self,
        public_key: &PublicKey,
        passphrase: &SecUtf8,
    ) -> Result<Option<PrivateKey>> {
        let private_key_bytes =
            self.storage
                .get_secure(KEYSPACE, public_key.serialize(), passphrase)?;

        private_key_bytes
            .map(|mut private_key_bytes| {
                let private_key = PrivateKey::deserialize_from(&private_key_bytes)?;
                private_key_bytes.zeroize();
                Ok(private_key)
            })
            .transpose()
    }

    /// Clears all storage
    pub fn clear(&self) -> Result<()> {
        self.storage.clear(KEYSPACE)
    }
}
