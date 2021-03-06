#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

#define SUCCESS 0

#define FAIL -1

typedef struct CroAddress CroAddress;

typedef struct CroDepositTx CroDepositTx;

typedef struct CroFee CroFee;

typedef struct CroHDWallet CroHDWallet;

typedef struct CroJsonRpc CroJsonRpc;

typedef struct CroTx CroTx;

typedef struct CroResult {
  int result;
} CroResult;

typedef CroAddress *CroAddressPtr;

typedef CroTx *CroTxPtr;

typedef CroHDWallet *CroHDWalletPtr;

typedef CroFee *CroFeePtr;

/**
 * current, start, end, userdata
 * return: 1: continue, 0: stop
 */
typedef int32_t (*ProgressCallback)(uint64_t, uint64_t, uint64_t, const void*);

typedef struct ProgressWrapper {
  ProgressCallback core_progress_callback;
} ProgressWrapper;

typedef ProgressWrapper *CroProgressPtr;

typedef CroJsonRpc *CroJsonRpcPtr;

/**
 * TODO: other states (jailed, unjail) will be added
 */
typedef struct CroStakedState {
  uint64_t nonce;
  uint64_t bonded;
  uint64_t unbonded;
  uint64_t unbonded_from;
} CroStakedState;

typedef CroDepositTx *CroDepositTxPtr;

/**
 * export privatekey as raw bytes
 * 32 bytes
 * # Safety
 */
CroResult cro_export_private(CroAddressPtr address_ptr, uint8_t *dst);

/**
 * get address as string
 * minimum byte length 100 is necessary
 * # Safety
 * address_output: string buffer, previously allocated
 */
CroResult cro_get_printed_address(CroAddressPtr address_ptr,
                                  uint8_t *address_output,
                                  uint32_t address_output_length);

/**
 * extract address as raw bytes
 * minimum 32 length is necessary
 * # Safety
 */
CroResult cro_extract_raw_address(CroAddressPtr address_ptr,
                                  uint8_t *address_output,
                                  uint32_t *address_output_length);

/**
 * create staking address
 * # Safety
 */
CroResult cro_basic_create_staking_address(CroAddressPtr *address_out);

/**
 * restore staking address
 * 32 bytes
 * # Safety
 */
CroResult cro_basic_restore_staking_address(CroAddressPtr *address_out, const uint8_t *input);

/**
 * create staking address
 * # Safety
 */
CroResult cro_basic_create_transfer_address(CroAddressPtr *address_out);

/**
 * restore transfer address
 * 32 bytes
 * # Safety
 */
CroResult cro_basic_restore_transfer_address(CroAddressPtr *address_out, const uint8_t *input);

/**
 * create viewkey, which is for encrypted tx
 * # Safety
 */
CroResult cro_basic_create_viewkey(CroAddressPtr *address_out);

/**
 * restore viewkey
 * 32 bytes
 * # Safety
 */
CroResult cro_basic_restore_viewkey(CroAddressPtr *address_out, const uint8_t *input);

/**
 * create tx
 * tx_out: previous allocated Tx
 * # Safety
 */
CroResult cro_create_tx(CroTxPtr *tx_out, uint8_t network);

/**
 * add txin
 * txid_string: null terminated string, 64 length hex-char , 32 bytes
 * addr_string: null terminated string, transfer address, ex) dcro1dfclvnmj77nfypp0na3ke2fl7nxe787aglynvr7hzvflukg34fqqnrnjek
 * coin: carson unit  for example) 1_0000_0000 carson = 1 cro, 1 carson = 0.0000_0001 cro
 * # Safety
 */
CroResult cro_tx_add_txin(CroTxPtr tx_ptr,
                          const char *txid_string,
                          uint16_t txindex,
                          const char *addr_string,
                          uint64_t coin);

/**
 * add txin in bytes
 * txid: txid in raw bytes, it's 32 bytes
 * txindex: which utxo in tx which txid_hex points
 * addr, coin: txid_hex + txindex points this utxo (address, coin value)
 * # Safety
 */
CroResult cro_tx_add_txin_raw(CroTxPtr tx_ptr,
                              const uint8_t *txid_user,
                              uint16_t txindex,
                              const uint8_t *addr_user,
                              uint64_t coin);

/**
 * add viewkey in string, which you can get from client-cli
 * viewkey_string: null terminated string
 * # Safety
 */
CroResult cro_tx_add_viewkey(CroTxPtr tx_ptr, const char *viewkey_string);

/**
 * add viewkey in bytes
 * viewkey: 33 raw bytes
 * # Safety
 */
CroResult cro_tx_add_viewkey_raw(CroTxPtr tx_ptr, const uint8_t *viewkey_user);

/**
 * extract bytes from signed tx
 * this output is encrypted with tx-query-app
 * can be broadcast to the network
 * output: raw bytes buffer, minimum 1000 bytes
 * # Safety
 */
CroResult cro_tx_complete_signing(CroTxPtr tx_ptr, uint8_t *output, uint32_t *output_length);

/**
 * sign for each txin
 * address_ptr: privatekey which will sign
 * tx_ptr: which tx to sign?
 * which_tx_in_user: which txin inside tx?
 * # Safety
 */
CroResult cro_tx_sign_txin(CroAddressPtr address_ptr, CroTxPtr tx_ptr, uint16_t which_tx_in_user);

/**
 * add txout , this makes utxo
 * addr_string: which address in string?
 * coin: value to send in carson unit , 1 carson= 0.0000_0001 cro
 * # Safety
 */
CroResult cro_tx_add_txout(CroTxPtr tx_ptr, const char *addr_string, uint64_t coin);

/**
 * add txout with bytes
 * addr: which address in bytes
 * coin: value to send in carson unit , 1 carson= 0.0000_0001 cro
 * # Safety
 */
CroResult cro_tx_add_txout_raw(CroTxPtr tx_ptr, const uint8_t *addr_user, uint64_t coin);

/**
 * destroy tx
 * # Safety
 */
CroResult cro_destroy_tx(CroTxPtr tx);

/**
 * create hd wallet
 * minimum  300 byte-length is necessary
 * # Safety
 */
CroResult cro_create_hdwallet(CroHDWalletPtr *wallet_out,
                              uint8_t *mnemonics,
                              uint32_t mnemonics_length);

/**
 * # Safety
 */
CroResult cro_restore_hdwallet(const char *mnemonics_string, CroHDWalletPtr *wallet_out);

/**
 * create staking address from bip44 hdwallet
 * # Safety
 */
CroResult cro_create_staking_address(CroHDWalletPtr wallet_ptr,
                                     Network network,
                                     CroAddressPtr *address_out,
                                     uint32_t index);

/**
 * create utxo address from bip44 wallet, which is for withdrawal, transfer amount
 * # Safety
 */
CroResult cro_create_transfer_address(CroHDWalletPtr wallet_ptr,
                                      Network network,
                                      CroAddressPtr *address_out,
                                      uint32_t index);

/**
 * create viewkey, which is for encrypted tx
 * # Safety
 */
CroResult cro_create_viewkey(CroHDWalletPtr wallet_ptr,
                             Network network,
                             CroAddressPtr *address_out,
                             uint32_t index);

/**
 * destroy bip44 hdwallet
 * # Safety
 * hdwallet: previously allocated hdwallet
 */
CroResult cro_destroy_hdwallet(CroHDWalletPtr hdwallet);

/**
 * destroy address
 * # Safety
 * addr: previously allocated address
 */
CroResult cro_destroy_address(CroAddressPtr addr);

/**
 * create fee algorithm
 * # Safety
 */
CroResult cro_create_fee_algorithm(CroFeePtr *fee_out,
                                   const char *constant_string,
                                   const char *coeff_string);

/**
 * estimate fee
 * tx_payload_size: in bytes
 * # Safety
 */
uint64_t cro_estimate_fee(CroFeePtr fee_ptr, uint32_t tx_payload_size);

/**
 * estimate fee after encryption
 * tx_payload_size: in bytes
 * # Safety
 */
uint64_t cro_estimate_fee_after_encrypt(CroFeePtr fee_ptr, uint32_t tx_payload_size);

/**
 * destroy fee
 * # Safety
 */
CroResult cro_destroy_fee_algorithm(CroFeePtr fee);

/**
 * # Safety
 *
 * Should not be called with null pointers.
 *
 * c example:
 *
 * ```c
 * char buf[BUFSIZE];
 * const char* req = "{\"jsonrpc\": \"2.0\", \"method\": \"wallet_list\", \"params\": [], \"id\": 1}";
 * int retcode = cro_jsonrpc_call("./data", "ws://...", 0xab, req, buf, sizeof(buf));
 * if (retcode == 0) {
 *     printf("response: %s\n", buf);
 * } else {
 *     printf("error: %s\n", buf);
 * }
 * ```
 */
CroResult cro_jsonrpc_call(const char *storage_dir,
                           const char *websocket_url,
                           uint8_t network_id,
                           const char *request,
                           char *buf,
                           uintptr_t buf_size,
                           CroProgressPtr progress_callback,
                           const void *user_data);

void cro_jsonrpc_call_dummy(ProgressCallback _progress_callback, ProgressWrapper _wrapper);

/**
 * create json-rpc context
 * rpc_out: null pointer which will be written
 * example c-code)
 *  CroJsonRpcPtr rpc= NULL;
 *  cro_create_jsonrpc(&rpc, ".storage", "ws://localhost:26657/websocket", 0xab, &progress);
 * storage_dir: ".storage"
 * websocket_url:  "ws://localhost:26657/websocket"
 * network: network-id  ex) 0xab
 * progress_callback: callback function which user codes
 * example c-code)
 * int32_t  progress(float rate)
 * {
 *    printf("progress %f\n", rate);
 * }
 * you can give this callback like below
 * CroResult retcode = cro_jsonrpc_call("./.storage", "ws://localhost:26657/websocket", 0xab, req, buf, sizeof(buf), &progress);
 * # Safety
 */
CroResult cro_create_jsonrpc(CroJsonRpcPtr *rpc_out,
                             const char *storage_dir_user,
                             const char *websocket_url_user,
                             uint8_t network_id,
                             CroProgressPtr progress_callback);

/**
 * request: json rpc request
 * example c code) const char* req = "{\"jsonrpc\": \"2.0\", \"method\": \"wallet_list\", \"params\": [], \"id\": 1}";
 * buf: minimum 500 bytes
 * buf_size: size of buf in bytes
 * # Safety
 */
CroResult cro_run_jsonrpc(CroJsonRpcPtr rpc_ptr,
                          const char *request,
                          char *buf,
                          uintptr_t buf_size,
                          const void *user_data);

/**
 * destroy json-rpc context
 * rpc: containing pointer to free
 * # Safety
 */
CroResult cro_destroy_jsonrpc(CroJsonRpcPtr rpc);

/**
 * staked -> staked
 * network: networkid
 * nonce: nonce of the staked state, use cro_get_staked_state to get this nonce
 * from_ptr: staking address
 * to_address_user:staking address, null terminated string ex) 0x1ad06eef15492a9a1ed0cfac21a1303198db8840
 * amount: carson unit   1 carson= 0.0000_0001 cro
 * output: signed tx encoded
 * # Safety
 */
CroResult cro_unbond(uint8_t network,
                     uint64_t nonce,
                     CroAddressPtr from_ptr,
                     const char *to_address_user,
                     uint64_t amount,
                     uint8_t *output,
                     uint32_t *output_length);

/**
 * staked -> utxo
 * tendermint_url_string:  "ws://localhost:26657/websocket"
 * network: network-id 0xab
 * from_ptr: staking address
 * to_address: transfer address   ex) dcro1xwjryuh85xejtc20qkwtesk0yjhfrwxjmwy6mmxjn0aevjhrw7dszselj5
 * viewkeys:hex encode null terminated string, viewkey list, this is string list   ex) 03fe7108a0c6f1dfae943d0193f56d6a5957cd391458d74016b8383c472c6c70d0
 * viewkey_count: number of viewkeys, this is count,   ex) if there is only 1 viewkey, viewkey_count is 1
 * output: minimum 1000 bytes, signed tx encoded
 * # Safety
 */
CroResult cro_withdraw(const char *tenermint_url_string,
                       uint8_t network,
                       CroAddressPtr from_ptr,
                       const char *to_address_string,
                       const char *const *viewkeys,
                       int32_t viewkey_count,
                       uint8_t *output,
                       uint32_t *output_length);

/**
 * staked -> utxo
 * from_ptr: staked address , previously allocated
 * tenermint_url_string: null terminated string ex) ws://localhost:26657/websocket
 * staked_state_user: previously allocated state ,retrieved state will be written
 * # Safety
 */
CroResult cro_get_staked_state(CroAddressPtr from_ptr,
                               const char *tenermint_url_string,
                               CroStakedState *staked_state_user);

/**
 * tendermint_url_string: default "ws://localhost:26657/websocket"
 * signed_transaction_user: signed tx encoded to encrypt
 * output: encrypted result will be written
 * # Safety
 */
CroResult cro_encrypt(const char *tenermint_url_string,
                      const uint8_t *signed_transaction_user,
                      uint32_t signed_transaction_length,
                      uint8_t *output,
                      uint32_t *output_length);

/**
 * staked -> utxo
 * tendermint_url: ws://localhost:26657/websocket
 * user_data: tx data to send
 * # Safety
 */
CroResult cro_broadcast(const char *tenermint_url_string,
                        const uint8_t *user_data,
                        uint32_t data_length);

/**
 * tx_ptr: tx TxoPointer
 * output: minimum 1000 bytes
 * output_length: actual tx length
 * # Safety
 */
CroResult cro_tx_complete_signing_deposit(CroDepositTxPtr tx_ptr,
                                          uint8_t *output,
                                          uint32_t *output_length);

/**
 * create deposit tx
 * network: network id  ex) 0xab
 * to_address_user: staking address, null terminated string  , ex) 0x1ad06eef15492a9a1ed0cfac21a1303198db8840
 * # Safety
 */
CroResult cro_create_tx_deposit(CroDepositTxPtr *tx_out,
                                uint8_t network,
                                const char *to_address_user);

/**
 * add txin
 * txid_string: 64 length hex-char , 32 bytes
 * addr_string: transfer address
 * coin: carson unit  for example) 1_0000_0000 carson = 1 cro, 1 carson = 0.0000_0001 cro
 * # Safety
 */
CroResult cro_tx_add_txin_deposit(CroDepositTxPtr tx_ptr,
                                  const char *txid_string,
                                  uint16_t txindex,
                                  const char *addr_string,
                                  uint64_t coin);

/**
 * sign for each txin
 * address_ptr: privatekey which will sign
 * tx_ptr: which tx to sign?
 * which_tx_in_user: which txin inside tx?
 * # Safety
 */
CroResult cro_tx_sign_txin_deposit(CroAddressPtr address_ptr,
                                   CroDepositTxPtr tx_ptr,
                                   uint16_t which_tx_in_user);

/**
 * destroy tx
 * tx: previously allocated tx
 * # Safety
 */
CroResult cro_destroy_tx_deposit(CroDepositTxPtr tx);

/**
 * staked -> staked
 * network: networkid   ex) 0xab
 * nonce: nonce of the staked state, use cro_get_staked_state to get this nonce
 * from_ptr: staking address
 * to_address_user:staking address, null terminated string   ex) 0x1ad06eef15492a9a1ed0cfac21a1303198db8840
 * output: signed tx encoded, minimum 1000 bytes
 * output_length: actual encoded length is returned
 * # Safety
 */
CroResult cro_unjai(uint8_t network,
                    uint64_t nonce,
                    CroAddressPtr from_ptr,
                    const char *to_address_user,
                    uint8_t *output,
                    uint32_t *output_length);

/**
 * staked -> staked
 * network: networkid    ex) 0xab
 * nonce: nonce of the staked state, use cro_get_staked_state to get this nonce
 * from_ptr: staking address
 * to_address_user:staking address, null terminated string
 * validator_name_user: validator name, null terminated string
 * validator_contact_user: validator contact, null terminated string
 * validator_pubkey_user: validator pubkey,ed25519 pubkey raw size= 32 bytes , base64 encoded  null terminated string,
 * output: signed tx encoded, minimum 1000 bytes
 * output_length: actual encoded length is returned
 * # Safety
 */
CroResult cro_join(uint8_t network,
                   uint64_t nonce,
                   CroAddressPtr from_ptr,
                   const char *to_address_user,
                   const char *validator_name_user,
                   const char *validator_contact_user,
                   const char *validator_pubkey_user,
                   const uint8_t *keypackage,
                   uintptr_t keypackage_len,
                   uint8_t *output,
                   uint32_t *output_length);
