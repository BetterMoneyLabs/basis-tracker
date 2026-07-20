// Regression test for the local-sign v3 ErgoTree parsing failure.
//
// `basis_cli transaction generate-redemption --local-sign` fails on reserve boxes whose
// ErgoTree uses header version 3 (the current `insertOrUpdate` Basis reserve contract):
//
//   Error: failed to parse reserve box 8dc21481...: ErgoTreeHeaderError(VersionError(InvalidVersion(3)))
//
// The failure happens in `sign_and_broadcast_local`
// (`crates/basis_cli/src/commands/transaction.rs`) when it parses the sigma-serialized
// input boxes with `ErgoBox::sigma_parse_bytes`, because ergo-lib 0.28 does not support
// ErgoTree header version 3.
//
// EXPECTED BEHAVIOR: parsing the real on-chain reserve box binary (below) must succeed so
// that client-side `proveDlog` signing works for v3 reserves.
//
// This test FAILS with ergo-lib 0.28 and should start passing once the v3 parsing issue
// is fixed (e.g. by an ergo-lib upgrade or a custom box-deserialization workaround).
// Until then, node-wallet signing (`/wallet/transaction/sign`) is the working path.

use ergo_lib::ergotree_ir::chain::ergo_box::ErgoBox;
use ergo_lib::ergotree_ir::serialization::SigmaSerializable;

/// Sigma-serialized bytes of the real mainnet reserve box
/// `8dc21481ed3f084f99d021124c9923e418c1ece60f2d44f8885d48923b29dcd0`
/// (v3 ErgoTree, `insertOrUpdate` Basis reserve contract), as returned by the Ergo node
/// `/utxo/byIdBinary` endpoint during the ninth redemption test.
const RESERVE_BOX_BINARY: &str = "8084af5f1bac052004140414050004000400041004200500040004420400040004000410050004420500040004420442010104e021050004020500058084af5f04040500040605000480a3050100d806d6017ee4e3000204d6029d72017300d603b2a59e7201730100d604e4c6a70407d605ededed93c27203c2a793db63087203db6308a793e4c672030407720493e4c67203060ee4c6a7060ed606e5c6a707057302959372027303d813d607b2db6501fe730400d608db07027204d609e4e30107d60acbb37208db07027209d60be4e30305d60ce4e30405d60de3070ed60ee6720dd60f7a720cd61095720e7cb4e4dc640ae4c6a7056402720ae4720d730573067307d61199c1a7c17203d612db6a01ddd613e4e3020ed614b4721373087309d615b3b3720a7a720b720fd616e4e3060ed617b17216d618917217730ad619e4c672070407ea02d1ededededededed7205938cb2db63087207730b0001e4c6a7060e937ce4dc640ae4c67207056402720ae4e3080e720b91720c95720e7cb4e4dc640ae4c6a7056402720ae4720d730c730d730e93e4dc6410e4c6a705640283013c0e0e8602720ab3720f7a9a72107211e4e3050ee4c672030564939f72127bb47213730fb17213a0ee72149f72047bcbb3b3721472157208eded917211731090721199720b7210957218957218d801d61ab4721673117312939f72127bb4721673137217a0ee721a9f72197bcbb3b3721a7215db0702721973149199a38cc7720701731593e5c67203070573167206cd7209959372027317d1ededed720593e4c672030564e4c6a7056493e5c672030705731872069299c17203c1a7731995937202731aea02d1edededed720592c17203c1a793e4c672030564e4c6a7056492e4c6720307057ea305937206731bcd720495937202731cea02d1ed917206731d927ea3059a72067e731e05cd7204d1731f80ee6f01018d29f4da1ea43f9d752b927200c54d9230637cc677c8a66d477f1684bd3098010407022880fde8cace85c2c810fb32c5441a32198b0f7a122b9a672cfb7e50eb898cdc64f91e7e764bd5cd630eb53d582a72a2f2bdaa1ca1459b2f6d79680bc8b1a32ebd010320000e20000b0695159e5f5c32c606385bd5f276d80133149c84c8b1325366381bf6f17f05004b65c7cdf46ba7fdb741803e5d6534de70911f07a409c285532821054cc2959b00";

#[test]
fn local_sign_reserve_box_v3_parses() {
    let bytes = hex::decode(RESERVE_BOX_BINARY).expect("valid hex");
    let parsed = ErgoBox::sigma_parse_bytes(&bytes);
    assert!(
        parsed.is_ok(),
        "local-sign must be able to parse v3 ErgoTree reserve boxes; \
         got error: {:?}",
        parsed.err()
    );
}
