//! Single-input transaction proving and redemption signing-message helpers.
//!
//! `Wallet::sign_transaction` proves *every* input and errors on any input it cannot prove, so it
//! cannot be used for the tracker-assisted 2-phase redemption where the tracker signs only the fee
//! input and the TUI signs only the reserve input. This module proves a *single* input over the
//! transaction's `bytes_to_sign` and splices the proof in, preserving every other input's proof.
//!
//! Neither caller reorders the reserve-input context extension: the tracker emits it in Scala
//! `ContextExtension` order, and both ends sign the identical `bytes_to_sign`.

use blake2::{Blake2b, Digest};
use generic_array::typenum::U32;
use thiserror::Error;

use ergo_lib::chain::transaction::unsigned::UnsignedTransaction;
use ergo_lib::chain::transaction::{Input, Transaction};
use ergo_lib::ergo_chain_types::{Header, PreHeader};
use ergo_lib::ergotree_interpreter::sigma_protocol::private_input::PrivateInput;
use ergo_lib::ergotree_interpreter::sigma_protocol::prover::hint::HintsBag;
use ergo_lib::ergotree_interpreter::sigma_protocol::prover::{ProofBytes, Prover, ProverResult};
use ergo_lib::ergotree_ir::chain::context::{Context, ContextExtensionProvider, TxIoVec};
use ergo_lib::ergotree_ir::chain::context_extension::ContextExtension;
use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::wallet::secret_key::SecretKey;

#[derive(Error, Debug)]
pub enum SigningError {
    #[error("input index {0} out of range")]
    InputIndex(usize),
    #[error("input box for index {0} not provided")]
    MissingBox(usize),
    #[error("bytes_to_sign failed: {0}")]
    BytesToSign(String),
    #[error("bounded vec: {0}")]
    BoundedVec(String),
    #[error("transaction build: {0}")]
    TxBuild(String),
    #[error("proving input {0} failed: {1}")]
    Prove(usize, String),
    #[error("not a dlog secret")]
    NotDlogSecret,
}

/// Redemption signing message: `blake2b256(issuer || receiver) || totalDebt(8 BE) || timestamp(8 BE)`.
pub fn redemption_signing_message(
    issuer_pk: &[u8; 33],
    receiver_pk: &[u8; 33],
    total_debt: u64,
    timestamp: u64,
) -> [u8; 48] {
    let mut h = Blake2b::<U32>::new();
    h.update(issuer_pk);
    h.update(receiver_pk);
    let key = h.finalize();
    let mut msg = [0u8; 48];
    msg[..32].copy_from_slice(&key);
    msg[32..40].copy_from_slice(&total_debt.to_be_bytes());
    msg[40..48].copy_from_slice(&timestamp.to_be_bytes());
    msg
}

/// Prove a single input of `unsigned_tx` with `secret` and return a signed `Transaction` whose
/// `input_idx` carries the produced proof while every other input keeps the proof it had in
/// `base_signed` (or `ProofBytes::Empty` when `base_signed` is `None`).
///
/// `input_boxes` are the spent boxes in transaction-input order; `data_boxes` are the data-input
/// boxes. `pre_header`/`headers` come from the same `ErgoStateContext` the caller uses for the tx.
#[allow(clippy::too_many_arguments)]
pub fn add_input_proof(
    unsigned_tx: &UnsignedTransaction,
    base_signed: Option<&Transaction>,
    input_boxes: &[ErgoBox],
    data_boxes: &[ErgoBox],
    pre_header: &PreHeader,
    headers: &[Header; 10],
    input_idx: usize,
    secret: &SecretKey,
) -> Result<Transaction, SigningError> {
    let n = unsigned_tx.inputs.len();
    if input_idx >= n {
        return Err(SigningError::InputIndex(input_idx));
    }
    let target_box = input_boxes
        .get(input_idx)
        .ok_or(SigningError::MissingBox(input_idx))?;
    if input_boxes.len() != n {
        return Err(SigningError::MissingBox(n));
    }

    let bytes_to_sign = unsigned_tx
        .bytes_to_sign()
        .map_err(|e| SigningError::BytesToSign(format!("{:?}", e)))?;

    // Real tx id (proofs do not affect it) via a throwaway empty-proof transaction, used to
    // materialize the output boxes the (reserve) contract reads from `OUTPUTS`.
    let empty_inputs = build_inputs(unsigned_tx, base_signed, None, input_boxes.len())?;
    let empty_tx = Transaction::new(
        TxIoVec::try_from(empty_inputs)
            .map_err(|e| SigningError::BoundedVec(format!("{:?}", e)))?,
        unsigned_tx.data_inputs.clone(),
        unsigned_tx.output_candidates.clone(),
    )
    .map_err(|e| SigningError::TxBuild(format!("{:?}", e)))?;
    let tx_id = empty_tx.id();

    let output_boxes: Vec<ErgoBox> = unsigned_tx
        .output_candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            ErgoBox::from_box_candidate(c, tx_id, i as u16)
                .map_err(|e| SigningError::TxBuild(format!("output {}: {:?}", i, e)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let private_input: PrivateInput = match secret {
        SecretKey::DlogSecretKey(d) => PrivateInput::DlogProverInput(d.clone()),
        _ => return Err(SigningError::NotDlogSecret),
    };
    let prover = ergo_lib::ergotree_interpreter::sigma_protocol::prover::TestProver {
        secrets: vec![private_input],
    };

    // Build the Context for the target input (self_box = target box, full inputs/outputs/data).
    let ctx = build_context(
        input_idx,
        target_box,
        input_boxes,
        &output_boxes,
        data_boxes,
        pre_header,
        headers,
        &unsigned_tx.inputs.get(input_idx).unwrap().extension,
    )?;

    let proof = prover
        .prove(
            &target_box.ergo_tree,
            &ctx,
            &bytes_to_sign,
            &HintsBag::empty(),
        )
        .map_err(|e| SigningError::Prove(input_idx, format!("{:?}", e)))?;

    let signed_inputs = build_inputs(
        unsigned_tx,
        base_signed,
        Some((input_idx, proof)),
        input_boxes.len(),
    )?;
    Transaction::new(
        TxIoVec::try_from(signed_inputs)
            .map_err(|e| SigningError::BoundedVec(format!("{:?}", e)))?,
        unsigned_tx.data_inputs.clone(),
        unsigned_tx.output_candidates.clone(),
    )
    .map_err(|e| SigningError::TxBuild(format!("{:?}", e)))
}

fn build_inputs(
    unsigned_tx: &UnsignedTransaction,
    base_signed: Option<&Transaction>,
    new_proof: Option<(usize, ProverResult)>,
    count: usize,
) -> Result<Vec<Input>, SigningError> {
    let mut inputs = Vec::with_capacity(count);
    for i in 0..count {
        let box_id = unsigned_tx.inputs.get(i).unwrap().box_id;
        let ext = unsigned_tx.inputs.get(i).unwrap().extension.clone();
        let input = if let Some((idx, ref p)) = new_proof {
            if i == idx {
                // interpreter ProverResult -> transaction ProverResult (private type) via Into
                Input::new(box_id, p.clone().into())
            } else if let Some(base) = base_signed.and_then(|b| b.inputs.get(i)) {
                Input::new(box_id, base.spending_proof.clone())
            } else {
                Input::new(
                    box_id,
                    ProverResult {
                        proof: ProofBytes::Empty,
                        extension: ext,
                    }
                    .into(),
                )
            }
        } else if let Some(base) = base_signed.and_then(|b| b.inputs.get(i)) {
            Input::new(box_id, base.spending_proof.clone())
        } else {
            Input::new(
                box_id,
                ProverResult {
                    proof: ProofBytes::Empty,
                    extension: ext,
                }
                .into(),
            )
        };
        inputs.push(input);
    }
    Ok(inputs)
}

/// No-op `ContextExtensionProvider`: the Basis reserve contract never reads other inputs'
/// context extensions (`getVarFromInput`), so returning `None` for every index is safe.
struct NoopExtensionProvider;

impl ContextExtensionProvider for NoopExtensionProvider {
    fn context_extension(&self, _input_index: usize) -> Option<&ContextExtension> {
        None
    }
}

static NOOP_EXTENSION_PROVIDER: NoopExtensionProvider = NoopExtensionProvider;

#[allow(clippy::too_many_arguments)]
fn build_context<'ctx>(
    _input_idx: usize,
    self_box: &'ctx ErgoBox,
    input_boxes: &'ctx [ErgoBox],
    output_boxes: &'ctx [ErgoBox],
    data_boxes: &'ctx [ErgoBox],
    pre_header: &PreHeader,
    headers: &[Header; 10],
    extension: &'ctx ContextExtension,
) -> Result<Context<'ctx>, SigningError> {
    let inputs: TxIoVec<&'ctx ErgoBox> = TxIoVec::try_from(input_boxes.iter().collect::<Vec<_>>())
        .map_err(|e| SigningError::BoundedVec(format!("{:?}", e)))?;
    let data_inputs: Option<TxIoVec<&'ctx ErgoBox>> = if data_boxes.is_empty() {
        None
    } else {
        Some(
            TxIoVec::try_from(data_boxes.iter().collect::<Vec<_>>())
                .map_err(|e| SigningError::BoundedVec(format!("{:?}", e)))?,
        )
    };
    Ok(Context {
        height: pre_header.height,
        self_box,
        outputs: output_boxes,
        data_inputs,
        inputs,
        pre_header: pre_header.clone(),
        headers: headers.clone(),
        extension,
        tree_version: Default::default(),
        extension_provider: &NOOP_EXTENSION_PROVIDER,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redemption_signing_message_is_48_bytes() {
        let issuer = [0x02u8; 33];
        let receiver = [0x03u8; 33];
        let msg = redemption_signing_message(&issuer, &receiver, 123, 456);
        assert_eq!(msg.len(), 48);
        assert_eq!(&msg[32..40], &123u64.to_be_bytes());
        assert_eq!(&msg[40..48], &456u64.to_be_bytes());
    }

    /// Cross-validation against the Scala reference implementation: test vector TV001 from
    /// specs/SCHNORR_SIGNATURE_SPEC.md (message = blake2b256(owner||receiver) || totalDebt BE
    /// || timestamp BE).
    #[test]
    fn redemption_signing_message_matches_scala_vector_tv001() {
        let issuer: [u8; 33] =
            hex::decode("0284bf7562262bbd6940085748f3be6afa52ae317155181ece31b66351ccffa4b0")
                .unwrap()
                .try_into()
                .unwrap();
        let receiver: [u8; 33] =
            hex::decode("02207bba70bc66309baa582a6ac120fd52d68026c51f6326f8ccedcbd2c1b7eb82")
                .unwrap()
                .try_into()
                .unwrap();
        let msg = redemption_signing_message(&issuer, &receiver, 1_000_000_000, 1_743_379_200_000);
        assert_eq!(
            hex::encode(&msg),
            "07b67390866bedf6c19b3fab1e29993ea6878e0d0dd0577ac6b6368c96a1220b000000003b9aca0000000195e97f7800"
        );
    }
}
