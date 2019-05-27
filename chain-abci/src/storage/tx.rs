use crate::storage::account::AccountStorage;
use crate::storage::account::AccountWrapper;
use crate::storage::{COL_BODIES, COL_TX_META};
use bit_vec::BitVec;
use chain_core::common::Timespec;
use chain_core::init::address::RedeemAddress;
use chain_core::init::coin::{Coin, CoinError};
use chain_core::state::account::Account;
use chain_core::state::account::{
    to_account_key, AccountOpWitness, DepositBondTx, UnbondTx, WithdrawUnbondedTx,
};
use chain_core::tx::data::input::TxoPointer;
use chain_core::tx::data::output::TxOut;
use chain_core::tx::data::TxId;
use chain_core::tx::fee::Fee;
use chain_core::tx::witness::TxWitness;
use chain_core::tx::TransactionId;
use chain_core::tx::{data::Tx, TxAux};
use kvdb::{DBTransaction, KeyValueDB};
use parity_codec::Decode;
use secp256k1;
use starling::constants::KEY_LEN;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::{fmt, io};

pub type StarlingFixedKey = [u8; KEY_LEN];

/// All possible TX validation errors
#[derive(Debug)]
pub enum Error {
    WrongChainHexId,
    NoInputs,
    NoOutputs,
    DuplicateInputs,
    ZeroCoin,
    InvalidSum(CoinError),
    UnexpectedWitnesses,
    MissingWitnesses,
    InvalidInput,
    InputSpent,
    InputOutputDoNotMatch,
    OutputInTimelock,
    EcdsaCrypto(secp256k1::Error),
    IoError(io::Error),
    AccountLookupError(starling::traits::Exception),
    AccountNotFound,
    AccountNotUnbonded,
    AccountWithdrawOutputNotLocked,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::Error::*;
        match self {
            WrongChainHexId => write!(f, "chain hex ID does not match"),
            DuplicateInputs => write!(f, "duplicated inputs"),
            UnexpectedWitnesses => write!(f, "transaction has more witnesses than inputs"),
            MissingWitnesses => write!(f, "transaction has more inputs than witnesses"),
            NoInputs => write!(f, "transaction has no inputs"),
            NoOutputs => write!(f, "transaction has no outputs"),
            ZeroCoin => write!(f, "output with no credited value"),
            InvalidSum(ref err) => write!(f, "input or output sum error: {}", err),
            InvalidInput => write!(f, "transaction spends an invalid input"),
            InputSpent => write!(f, "transaction spends an input that was already spent"),
            InputOutputDoNotMatch => write!(
                f,
                "transaction input output coin (plus fee) sums don't match"
            ),
            OutputInTimelock => write!(f, "output transaction is in timelock"),
            EcdsaCrypto(ref err) => write!(f, "ECDSA crypto error: {}", err),
            IoError(ref err) => write!(f, "IO error: {}", err),
            AccountLookupError(ref err) => write!(f, "Account lookup error: {}", err),
            AccountNotFound => write!(f, "account not found"),
            AccountNotUnbonded => write!(f, "account not unbonded for withdrawal"),
            AccountWithdrawOutputNotLocked => write!(
                f,
                "account withdrawal outputs not time-locked to unbonded_from"
            ),
        }
    }
}

/// Given a db and a DB transaction, it will go through TX inputs and mark them as spent
/// in the TX_META storage.
pub fn spend_utxos(tx: &Tx, db: Arc<dyn KeyValueDB>, dbtx: &mut DBTransaction) {
    let mut updated_txs = BTreeMap::new();
    for txin in tx.inputs.iter() {
        updated_txs
            .entry(txin.id)
            .or_insert_with(|| {
                BitVec::from_bytes(&db.get(COL_TX_META, &txin.id[..]).unwrap().unwrap())
            })
            .set(txin.index, true);
    }
    for (txid, bv) in &updated_txs {
        dbtx.put(COL_TX_META, &txid[..], &bv.to_bytes());
    }
}

/// Given a db and a DB transaction, it will go through TX inputs and mark them as spent
/// in the TX_META storage and it will create a new entry for TX in TX_META with all outputs marked as unspent.
pub fn update_utxos_commit(tx: &Tx, db: Arc<dyn KeyValueDB>, dbtx: &mut DBTransaction) {
    spend_utxos(tx, db, dbtx);
    dbtx.put(
        COL_TX_META,
        &tx.id(),
        &BitVec::from_elem(tx.outputs.len(), false).to_bytes(),
    );
}

/// External information needed for TX validation
#[derive(Clone, Copy)]
pub struct ChainInfo {
    pub min_fee_computed: Fee,
    pub chain_hex_id: u8,
    pub previous_block_time: Timespec,
    pub last_account_root_hash: StarlingFixedKey,
}

fn check_attributes(tx_chain_hex_id: u8, extra_info: &ChainInfo) -> Result<(), Error> {
    // TODO: check other attributes?
    // check that chain IDs match
    if extra_info.chain_hex_id != tx_chain_hex_id {
        return Err(Error::WrongChainHexId);
    }
    Ok(())
}

fn check_inputs_basic(inputs: &[TxoPointer], witness: &TxWitness) -> Result<(), Error> {
    // check that there are inputs
    if inputs.is_empty() {
        return Err(Error::NoInputs);
    }

    // check that there are no duplicate inputs
    let mut inputs_s = BTreeSet::new();
    if !inputs.iter().all(|x| inputs_s.insert(x)) {
        return Err(Error::DuplicateInputs);
    }

    // verify transaction witnesses
    if inputs.len() < witness.len() {
        return Err(Error::UnexpectedWitnesses);
    }

    if inputs.len() > witness.len() {
        return Err(Error::MissingWitnesses);
    }

    Ok(())
}

fn check_inputs_lookup(
    main_txid: &TxId,
    inputs: &[TxoPointer],
    witness: &TxWitness,
    extra_info: &ChainInfo,
    db: Arc<dyn KeyValueDB>,
) -> Result<Coin, Error> {
    let mut incoins = Coin::zero();
    // verify that txids of inputs correspond to the owner/signer
    // and it'd check they are not spent
    for (txin, in_witness) in inputs.iter().zip(witness.iter()) {
        let txo = db.get(COL_TX_META, &txin.id[..]);
        match txo {
            Ok(Some(v)) => {
                let bv = BitVec::from_bytes(&v).get(txin.index);
                if bv.is_none() {
                    return Err(Error::InvalidInput);
                }
                if bv.unwrap() {
                    return Err(Error::InputSpent);
                }
                let txdata = db.get(COL_BODIES, &txin.id[..]).unwrap().unwrap().to_vec();
                let tx = Tx::decode(&mut txdata.as_slice()).unwrap();
                if txin.index >= tx.outputs.len() {
                    return Err(Error::InvalidInput);
                }
                let txout = &tx.outputs[txin.index];
                if let Some(valid_from) = &txout.valid_from {
                    if *valid_from > extra_info.previous_block_time {
                        return Err(Error::OutputInTimelock);
                    }
                }

                let wv = in_witness.verify_tx_address(main_txid, &txout.address);
                if let Err(e) = wv {
                    return Err(Error::EcdsaCrypto(e));
                }
                let sum = incoins + txout.value;
                if let Err(e) = sum {
                    return Err(Error::InvalidSum(e));
                } else {
                    incoins = sum.unwrap();
                }
            }
            Ok(None) => {
                return Err(Error::InvalidInput);
            }
            Err(e) => {
                return Err(Error::IoError(e));
            }
        }
    }
    Ok(incoins)
}

fn check_outputs_basic(outputs: &[TxOut]) -> Result<(), Error> {
    // check that there are outputs
    if outputs.is_empty() {
        return Err(Error::NoOutputs);
    }

    // check that all outputs have a non-zero amount
    if !outputs.iter().all(|x| x.value > Coin::zero()) {
        return Err(Error::ZeroCoin);
    }

    // Note: we don't need to check against MAX_COIN because Coin's
    // constructor should already do it.

    // TODO: check address attributes?
    Ok(())
}

fn check_input_output_sums(
    incoins: Coin,
    outcoins: Coin,
    extra_info: &ChainInfo,
) -> Result<Coin, Error> {
    // check sum(input amounts) >= sum(output amounts) + minimum fee
    let min_fee: Coin = extra_info.min_fee_computed.to_coin();
    let total_outsum = outcoins + min_fee;
    if let Err(coin_err) = total_outsum {
        return Err(Error::InvalidSum(coin_err));
    }
    if incoins < total_outsum.unwrap() {
        return Err(Error::InputOutputDoNotMatch);
    }
    let fee_paid = (incoins - outcoins).unwrap();
    Ok(fee_paid)
}

/// checks TransferTx -- TODO: this will be moved to an enclave
/// TODO: when more address/sigs available, check Redeem addresses are never in outputs?
fn verify_transfer(
    maintx: &Tx,
    witness: &TxWitness,
    extra_info: ChainInfo,
    db: Arc<dyn KeyValueDB>,
) -> Result<Coin, Error> {
    check_attributes(maintx.attributes.chain_hex_id, &extra_info)?;
    check_inputs_basic(&maintx.inputs, witness)?;
    check_outputs_basic(&maintx.outputs)?;
    let incoins = check_inputs_lookup(&maintx.id(), &maintx.inputs, witness, &extra_info, db)?;
    let outcoins = maintx.get_output_total();
    if let Err(coin_err) = outcoins {
        return Err(Error::InvalidSum(coin_err));
    }
    check_input_output_sums(incoins, outcoins.unwrap(), &extra_info)
}

fn verify_bonded_deposit(
    maintx: &DepositBondTx,
    witness: &TxWitness,
    extra_info: ChainInfo,
    db: Arc<dyn KeyValueDB>,
    _accounts: &AccountStorage,
) -> Result<Coin, Error> {
    check_attributes(maintx.attributes.chain_hex_id, &extra_info)?;
    check_inputs_basic(&maintx.inputs, witness)?;
    let incoins = check_inputs_lookup(&maintx.id(), &maintx.inputs, witness, &extra_info, db)?;
    check_input_output_sums(incoins, maintx.value, &extra_info)
    // TODO: check account not jailed etc.?
}

/// checks that the account can be retrieved from the trie storage
fn get_account(
    account_address: &RedeemAddress,
    extra_info: &ChainInfo,
    accounts: &AccountStorage,
) -> Result<Account, Error> {
    let account_key = to_account_key(account_address);
    let items = accounts.get(&extra_info.last_account_root_hash, &mut [&account_key]);
    if let Err(e) = items {
        return Err(Error::AccountLookupError(e));
    }
    let account = items.unwrap()[&account_key].clone();
    match account {
        None => Err(Error::AccountNotFound),
        Some(AccountWrapper(a)) => Ok(a),
    }
}

fn verify_unbonding(
    maintx: &UnbondTx,
    witness: &AccountOpWitness,
    extra_info: ChainInfo,
    accounts: &AccountStorage,
) -> Result<Coin, Error> {
    check_attributes(maintx.attributes.chain_hex_id, &extra_info)?;
    let account_address = witness.verify_tx_recover_address(&maintx.id());
    if let Err(e) = account_address {
        return Err(Error::EcdsaCrypto(e));
    }
    let account = get_account(&account_address.unwrap(), &extra_info, accounts)?;
    check_input_output_sums(account.bonded, maintx.value, &extra_info)
}

fn verify_unbonded_withdraw(
    maintx: &WithdrawUnbondedTx,
    witness: &AccountOpWitness,
    extra_info: ChainInfo,
    accounts: &AccountStorage,
) -> Result<Coin, Error> {
    check_attributes(maintx.attributes.chain_hex_id, &extra_info)?;
    check_outputs_basic(&maintx.outputs)?;
    let account_address = witness.verify_tx_recover_address(&maintx.id());
    if let Err(e) = account_address {
        return Err(Error::EcdsaCrypto(e));
    }
    let account = get_account(&account_address.unwrap(), &extra_info, accounts)?;
    // checks that account can withdraw to outputs
    if account.unbonded_from > extra_info.previous_block_time {
        return Err(Error::AccountNotUnbonded);
    }
    // checks that outputs are locked to the unbonded time
    if !maintx
        .outputs
        .iter()
        .all(|x| x.valid_from == Some(account.unbonded_from))
    {
        return Err(Error::AccountWithdrawOutputNotLocked);
    }
    let outcoins = maintx.get_output_total();
    if let Err(coin_err) = outcoins {
        return Err(Error::InvalidSum(coin_err));
    }
    check_input_output_sums(account.unbonded, outcoins.unwrap(), &extra_info)
}

/// Checks TX against the current DB and returns an `Error` if something fails.
/// If OK, returns the paid fee.
pub fn verify(
    txaux: &TxAux,
    extra_info: ChainInfo,
    db: Arc<dyn KeyValueDB>,
    accounts: &AccountStorage,
) -> Result<Fee, Error> {
    let paid_fee = match txaux {
        TxAux::TransferTx(maintx, witness) => verify_transfer(maintx, witness, extra_info, db)?,
        TxAux::DepositStakeTx(maintx, witness) => {
            verify_bonded_deposit(maintx, witness, extra_info, db, accounts)?
        }
        TxAux::UnbondStakeTx(maintx, witness) => {
            verify_unbonding(maintx, witness, extra_info, accounts)?
        }
        TxAux::WithdrawUnbondedStakeTx(maintx, witness) => {
            verify_unbonded_withdraw(maintx, witness, extra_info, accounts)?
        }
    };
    Ok(Fee::new(paid_fee))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::storage::{Storage, COL_TX_META, NUM_COLUMNS};
    use chain_core::init::address::RedeemAddress;
    use chain_core::tx::data::{address::ExtendedAddr, input::TxoPointer, output::TxOut};
    use chain_core::tx::fee::FeeAlgorithm;
    use chain_core::tx::fee::{LinearFee, Milli};
    use chain_core::tx::witness::{TxInWitness, TxWitness};
    use kvdb_memorydb::create;
    use parity_codec::Encode;
    use secp256k1::{key::PublicKey, key::SecretKey, Message, Secp256k1, Signing};
    use std::fmt::Debug;
    use std::mem;

    pub fn get_tx_witness<C: Signing>(
        secp: Secp256k1<C>,
        tx: &Tx,
        secret_key: &SecretKey,
    ) -> TxInWitness {
        let message = Message::from_slice(&tx.id()[..]).expect("32 bytes");
        let sig = secp.sign_recoverable(&message, &secret_key);
        return TxInWitness::BasicRedeem(sig);
    }

    fn create_db() -> Arc<dyn KeyValueDB> {
        Arc::new(create(NUM_COLUMNS.unwrap()))
    }

    fn prepare_app_valid_tx(
        timelocked: bool,
    ) -> (
        Arc<dyn KeyValueDB>,
        TxAux,
        Tx,
        TxWitness,
        SecretKey,
        AccountStorage,
    ) {
        let db = create_db();

        let mut tx = Tx::new();
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);

        let addr = ExtendedAddr::BasicRedeem(RedeemAddress::from(&public_key));
        let mut old_tx = Tx::new();

        if timelocked {
            old_tx.add_output(TxOut::new_with_timelock(addr.clone(), Coin::one(), 20));
        } else {
            old_tx.add_output(TxOut::new_with_timelock(addr.clone(), Coin::one(), -20));
        }

        let old_tx_id = old_tx.id();
        let txp = TxoPointer::new(old_tx_id, 0);

        let mut inittx = db.transaction();
        inittx.put(COL_BODIES, &old_tx_id[..], &old_tx.encode());

        inittx.put(
            COL_TX_META,
            &old_tx_id[..],
            &BitVec::from_elem(1, false).to_bytes(),
        );
        db.write(inittx).unwrap();
        tx.add_input(txp);
        tx.add_output(TxOut::new(addr, Coin::new(9).unwrap()));
        let sk2 = SecretKey::from_slice(&[0x11; 32]).expect("32 bytes, within curve order");
        let pk2 = PublicKey::from_secret_key(&secp, &sk2);
        tx.add_output(TxOut::new(
            ExtendedAddr::BasicRedeem(RedeemAddress::from(&pk2)),
            Coin::new(1).unwrap(),
        ));

        let witness: Vec<TxInWitness> = vec![get_tx_witness(secp, &tx, &secret_key)];
        let txaux = TxAux::new(tx.clone(), witness.clone().into());
        (
            db,
            txaux,
            tx.clone(),
            witness.into(),
            secret_key,
            AccountStorage::new(Storage::new_db(create_db()), 20).expect("account db"),
        )
    }

    const DEFAULT_CHAIN_ID: u8 = 0;

    #[test]
    fn existing_utxo_input_tx_should_verify() {
        let (db, txaux, _, _, _, accounts) = prepare_app_valid_tx(false);
        let extra_info = ChainInfo {
            min_fee_computed: LinearFee::new(Milli::new(1, 1), Milli::new(1, 1))
                .calculate_for_txaux(&txaux)
                .expect("invalid fee policy"),
            chain_hex_id: DEFAULT_CHAIN_ID,
            previous_block_time: 0,
            last_account_root_hash: [0u8; 32],
        };
        let result = verify(&txaux, extra_info, db, &accounts);
        assert!(result.is_ok());
    }

    fn expect_error<T, Error>(res: &Result<T, Error>, expected: Error)
    where
        Error: Debug,
    {
        match res {
            Err(err) if mem::discriminant(&expected) == mem::discriminant(err) => {}
            Err(err) => panic!("Expected error {:?} but got {:?}", expected, err),
            Ok(_) => panic!("Expected error {:?} but succeeded", expected),
        }
    }

    #[test]
    fn test_verify_fail() {
        let (db, txaux, tx, witness, secret_key, accounts) = prepare_app_valid_tx(false);
        let extra_info = ChainInfo {
            min_fee_computed: LinearFee::new(Milli::new(1, 1), Milli::new(1, 1))
                .calculate_for_txaux(&txaux)
                .expect("invalid fee policy"),
            chain_hex_id: DEFAULT_CHAIN_ID,
            previous_block_time: 0,
            last_account_root_hash: [0u8; 32],
        };
        // WrongChainHexId
        {
            let mut extra_info = extra_info.clone();
            extra_info.chain_hex_id = DEFAULT_CHAIN_ID + 1;
            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(&result, Error::WrongChainHexId);
        }
        // NoInputs
        {
            let mut tx = tx.clone();
            tx.inputs.clear();
            let txaux = TxAux::TransferTx(tx, witness.clone());
            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(&result, Error::NoInputs);
        }
        // NoOutputs
        {
            let mut tx = tx.clone();
            tx.outputs.clear();
            let txaux = TxAux::TransferTx(tx, witness.clone());
            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(&result, Error::NoOutputs);
        }
        // DuplicateInputs
        {
            let mut tx = tx.clone();
            let inp = tx.inputs[0].clone();
            tx.inputs.push(inp);
            let txaux = TxAux::TransferTx(tx, witness.clone());
            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(&result, Error::DuplicateInputs);
        }
        // ZeroCoin
        {
            let mut tx = tx.clone();
            tx.outputs[0].value = Coin::zero();
            let txaux = TxAux::TransferTx(tx, witness.clone());
            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(&result, Error::ZeroCoin);
        }
        // UnexpectedWitnesses
        {
            let mut witness = witness.clone();
            let wp = witness[0].clone();
            witness.push(wp);
            let txaux = TxAux::TransferTx(tx.clone(), witness);
            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(&result, Error::UnexpectedWitnesses);
        }
        // MissingWitnesses
        {
            let txaux = TxAux::TransferTx(tx.clone(), vec![].into());
            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(&result, Error::MissingWitnesses);
        }
        // InvalidSum
        {
            let mut tx = tx.clone();
            tx.outputs[0].value = Coin::max();
            let outp = tx.outputs[0].clone();
            tx.outputs.push(outp);
            let mut witness = witness.clone();
            witness[0] = get_tx_witness(Secp256k1::new(), &tx, &secret_key);
            let txaux = TxAux::TransferTx(tx, witness);
            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(
                &result,
                Error::InvalidSum(CoinError::OutOfBound(Coin::max().into())),
            );
        }
        // InputSpent
        {
            let mut inittx = db.transaction();
            inittx.put(
                COL_TX_META,
                &tx.inputs[0].id[..],
                &BitVec::from_elem(1, true).to_bytes(),
            );
            db.write(inittx).unwrap();

            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(&result, Error::InputSpent);

            let mut reset = db.transaction();
            reset.put(
                COL_TX_META,
                &tx.inputs[0].id[..],
                &BitVec::from_elem(1, false).to_bytes(),
            );
            db.write(reset).unwrap();
        }
        // Invalid signature (EcdsaCrypto)
        {
            let secp = Secp256k1::new();
            let mut witness = witness.clone();
            witness[0] = get_tx_witness(
                secp.clone(),
                &tx,
                &SecretKey::from_slice(&[0x11; 32]).expect("32 bytes, within curve order"),
            );
            let txaux = TxAux::TransferTx(tx.clone(), witness);
            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(
                &result,
                Error::EcdsaCrypto(secp256k1::Error::InvalidPublicKey),
            );
        }
        // InvalidInput
        {
            let result = verify(&txaux, extra_info, create_db(), &accounts);
            expect_error(&result, Error::InvalidInput);
        }
        // InputOutputDoNotMatch
        {
            let mut tx = tx.clone();
            let mut witness = witness.clone();

            tx.outputs[0].value = (tx.outputs[0].value + Coin::one()).unwrap();
            witness[0] = get_tx_witness(Secp256k1::new(), &tx, &secret_key);
            let txaux = TxAux::TransferTx(tx, witness);
            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(&result, Error::InputOutputDoNotMatch);
        }
        // OutputInTimelock
        {
            let (db, txaux, _, _, _, accounts) = prepare_app_valid_tx(true);
            let result = verify(&txaux, extra_info, db.clone(), &accounts);
            expect_error(&result, Error::OutputInTimelock);
        }
    }

}
