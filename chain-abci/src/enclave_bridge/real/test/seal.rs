use crate::enclave_bridge::real::enclave_u::{check_initchain, check_tx, end_block};
use crate::enclave_bridge::real::TX_VALIDATION_ENCLAVE_FILE;
use chain_core::common::MerkleTree;
use chain_core::init::address::RedeemAddress;
use chain_core::init::coin::Coin;
use chain_core::state::account::{
    StakedState, StakedStateAddress, StakedStateOpWitness, WithdrawUnbondedTx,
};
use chain_core::state::tendermint::BlockHeight;
use chain_core::tx::fee::Fee;
use chain_core::tx::witness::tree::RawXOnlyPubkey;
use chain_core::tx::witness::EcdsaSignature;
use chain_core::tx::TransactionId;
use chain_core::tx::TxObfuscated;
use chain_core::tx::{
    data::{
        access::{TxAccess, TxAccessPolicy},
        address::ExtendedAddr,
        attribute::TxAttributes,
        input::{TxoPointer, TxoSize},
        output::TxOut,
        Tx, TxId,
    },
    witness::TxInWitness,
    TxEnclaveAux,
};
use chain_core::ChainInfo;
use chain_tx_validation::Error;
use enclave_protocol::{
    EncryptionRequest, IntraEnclaveRequest, IntraEnclaveResponseOk, VerifyTxRequest,
};
use enclave_u_common::enclave_u::init_enclave;
use env_logger::{Builder, WriteStyle};
use log::LevelFilter;
use log::{debug, error, info};
use parity_scale_codec::{Decode, Encode};
use secp256k1::{
    key::PublicKey, key::SecretKey, key::XOnlyPublicKey, schnorrsig::schnorr_sign, Message,
    Secp256k1, Signing,
};
use sgx_types::{sgx_enclave_id_t, sgx_status_t};

extern "C" {
    fn ecall_test_encrypt(
        eid: sgx_enclave_id_t,
        retval: *mut sgx_status_t,
        enc_request: *const u8,
        enc_request_len: usize,
        response_buf: *mut u8,
        response_len: u32,
    ) -> sgx_status_t;
}

pub fn encrypt(eid: sgx_enclave_id_t, request: EncryptionRequest) -> TxObfuscated {
    let request_buf: Vec<u8> = request.encode();
    let response_len = 2 * request_buf.len();
    let mut response_buf: Vec<u8> = vec![0u8; response_len];
    let mut retval: sgx_status_t = sgx_status_t::SGX_SUCCESS;
    let response_slice = &mut response_buf[..];
    let result = unsafe {
        ecall_test_encrypt(
            eid,
            &mut retval,
            request_buf.as_ptr(),
            request_buf.len(),
            response_slice.as_mut_ptr(),
            response_buf.len() as u32,
        )
    };
    if retval == sgx_status_t::SGX_SUCCESS && result == retval {
        TxObfuscated::decode(&mut response_buf.as_slice()).expect("test response")
    } else {
        panic!("test enclave call failed: {} {}", retval, result);
    }
}

fn get_ecdsa_witness<C: Signing>(
    secp: &Secp256k1<C>,
    txid: &TxId,
    secret_key: &SecretKey,
) -> EcdsaSignature {
    let message = Message::from_slice(&txid[..]).expect("32 bytes");
    let sig = secp.sign_recoverable(&message, &secret_key);
    return sig;
}

fn get_account(account_address: &RedeemAddress) -> StakedState {
    let mut state = StakedState::default(StakedStateAddress::from(*account_address));
    state.unbonded = Coin::one();
    state
}

const TEST_NETWORK_ID: u8 = 0xab;

/// Unfortunately the usual Rust unit-test facility can't be used with Apache Teaclave SGX SDK,
/// so this has to be run as a normal app
pub fn test_sealing() {
    let mut builder = Builder::new();

    builder
        .filter(None, LevelFilter::Debug)
        .write_style(WriteStyle::Always)
        .init();

    let enclave = match init_enclave(TX_VALIDATION_ENCLAVE_FILE, true) {
        Ok(r) => {
            info!("[+] Init Enclave Successful {}!", r.geteid());
            r
        }
        Err(x) => {
            error!("[-] Init Enclave Failed {}!", x.as_str());
            return;
        }
    };
    assert!(check_initchain(enclave.geteid(), TEST_NETWORK_ID).is_ok());

    let end_b = end_block(enclave.geteid(), IntraEnclaveRequest::EndBlock);
    match end_b {
        Ok(IntraEnclaveResponseOk::EndBlock(b)) => {
            debug!("request filter in the beginning");
            assert!(b.is_none(), "empty filter");
        }
        _ => {
            assert!(false, "filter not returned");
        }
    };

    let secp = Secp256k1::new();
    let secret_key = SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order");
    let public_key = PublicKey::from_secret_key(&secp, &secret_key);
    let x_public_key = XOnlyPublicKey::from_secret_key(&secp, &secret_key);

    let addr = RedeemAddress::from(&public_key);

    let merkle_tree = MerkleTree::new(vec![RawXOnlyPubkey::from(x_public_key.serialize())]);

    let eaddr = ExtendedAddr::OrTree(merkle_tree.root_hash());
    let tx0 = WithdrawUnbondedTx::new(
        0,
        vec![TxOut::new_with_timelock(eaddr.clone(), Coin::one(), 0)],
        TxAttributes::new_with_access(
            TEST_NETWORK_ID,
            vec![TxAccessPolicy::new(public_key.clone(), TxAccess::AllData)],
        ),
    );
    let txid = &tx0.id();
    let witness0 = StakedStateOpWitness::new(get_ecdsa_witness(&secp, &txid, &secret_key));
    let account = get_account(&addr);
    let withdrawtx = TxEnclaveAux::WithdrawUnbondedStakeTx {
        no_of_outputs: tx0.outputs.len() as TxoSize,
        witness: witness0.clone(),
        payload: encrypt(
            enclave.geteid(),
            EncryptionRequest::WithdrawStake(tx0, Box::new(account.clone()), witness0),
        ),
    };

    let info = ChainInfo {
        min_fee_computed: Fee::new(Coin::zero()),
        chain_hex_id: TEST_NETWORK_ID,
        block_time: 1,
        block_height: BlockHeight::genesis(),
        unbonding_period: 0,
    };

    let request0 = IntraEnclaveRequest::ValidateTx {
        request: Box::new(VerifyTxRequest {
            tx: withdrawtx,
            account: Some(account),
            info,
        }),
        tx_inputs: None,
    };
    let r = check_tx(enclave.geteid(), request0).unwrap();

    let sealedtx = match r {
        IntraEnclaveResponseOk::TxWithOutputs { sealed_tx, .. } => sealed_tx,
        _ => vec![],
    };

    let end_b = end_block(enclave.geteid(), IntraEnclaveRequest::EndBlock);
    match end_b {
        Ok(IntraEnclaveResponseOk::EndBlock(b)) => {
            debug!("request filter after one tx");
            assert!(b.unwrap().iter().any(|x| *x != 0u8), "non-empty filter");
        }
        _ => {
            assert!(false, "filter not returned");
        }
    };

    let halfcoin = Coin::from(5000_0000u32);
    let utxo1 = TxoPointer::new(*txid, 0);
    let mut tx1 = Tx::new();
    tx1.attributes = TxAttributes::new(TEST_NETWORK_ID);
    tx1.add_input(utxo1.clone());
    tx1.add_output(TxOut::new(eaddr.clone(), halfcoin));
    let txid1 = tx1.id();
    let witness1 = vec![TxInWitness::TreeSig(
        schnorr_sign(&secp, &Message::from_slice(&txid1).unwrap(), &secret_key),
        merkle_tree
            .generate_proof(RawXOnlyPubkey::from(x_public_key.serialize()))
            .unwrap(),
    )]
    .into();
    let transfertx = TxEnclaveAux::TransferTx {
        inputs: tx1.inputs.clone(),
        no_of_outputs: tx1.outputs.len() as TxoSize,
        payload: encrypt(
            enclave.geteid(),
            EncryptionRequest::TransferTx(tx1, witness1),
        ),
    };

    let request1 = IntraEnclaveRequest::ValidateTx {
        request: Box::new(VerifyTxRequest {
            tx: transfertx,
            account: None,
            info,
        }),
        tx_inputs: Some(vec![sealedtx.clone()]),
    };

    check_tx(enclave.geteid(), request1).unwrap();

    let mut tx2 = Tx::new();
    tx2.attributes = TxAttributes::new(TEST_NETWORK_ID);
    tx2.add_input(utxo1);
    tx2.add_output(TxOut::new(eaddr.clone(), Coin::zero()));
    let txid2 = tx2.id();
    let witness2 = vec![TxInWitness::TreeSig(
        schnorr_sign(&secp, &Message::from_slice(&txid2).unwrap(), &secret_key),
        merkle_tree
            .generate_proof(RawXOnlyPubkey::from(x_public_key.serialize()))
            .unwrap(),
    )]
    .into();
    let transfertx2 = TxEnclaveAux::TransferTx {
        inputs: tx2.inputs.clone(),
        no_of_outputs: tx2.outputs.len() as TxoSize,
        payload: encrypt(
            enclave.geteid(),
            EncryptionRequest::TransferTx(tx2, witness2),
        ),
    };
    let request2 = IntraEnclaveRequest::ValidateTx {
        request: Box::new(VerifyTxRequest {
            tx: transfertx2,
            account: None,
            info,
        }),
        tx_inputs: Some(vec![sealedtx]),
    };

    let r3 = check_tx(enclave.geteid(), request2);
    match r3 {
        Err(Error::ZeroCoin) => {
            debug!("invalid transaction rejected and error code returned");
        }
        Err(x) => {
            panic!(
                "something else happened (tx not correctly rejected): {:?}",
                x
            );
        }
        Ok(_) => {
            panic!("something else happened (tx accepted)");
        }
    };
}
