package basis.contracts

import basis.offchain.SigUtils
import com.google.common.primitives.Longs
import org.ergoplatform.appkit.{BlockchainContext, ConstantsBuilder, ContextVar, ErgoValue, InputBox}
import org.ergoplatform.sdk.ErgoToken
import org.scalatest.{Matchers, PropSpec}
import scorex.crypto.hash.Blake2b256
import sigma.AvlTree
import sigma.data.AvlTreeFlags
import work.lithos.plasma.PlasmaParameters
import work.lithos.plasma.collections.PlasmaMap

/**
  * Security regression tests for `contract/basis-token.es` (PR #14 C6).
  *
  * The token reserve pays out reserve TOKENS, never ERG. Before the fix, neither action #0 (redemption)
  * nor action #1 (top-up) constrained `selfOut.value`:
  *
  *   - action #1 requires NO signature, so anyone could add 1 token unit and take the whole ERG
  *     balance sitting in the reserve box;
  *   - action #0 computed the redeemed amount from the token delta alone
  *     (`redeemed = tokenAmountIn - tokenAmountOut`), leaving ERG unconstrained during a redemption.
  *
  * The fix adds `ergPreserved = selfOut.value >= SELF.value` to both actions, and pins the reserve-NFT
  * amount so an output cannot carry more units than the input (which would mint reserve NFTs).
  *
  * Every property here asserts the attack is REJECTED, against the real `contract/basis-token.es`.
  */
class BasisTokenSecuritySpec extends PropSpec with Matchers with basis.HttpClientTesting {

  private val t = new BasisTokenSpec

  private val reserveParams = PlasmaParameters(32, None)
  private val reserveFlags = AvlTreeFlags(insertAllowed = true, updateAllowed = true, removeAllowed = false)

  private def freshReserveMap = new PlasmaMap[Array[Byte], Array[Byte]](reserveFlags, reserveParams)

  private def reserveValue(ts: Long, redeemed: Long): Array[Byte] =
    Longs.toByteArray(ts) ++ Longs.toByteArray(redeemed)

  private def signAll(message: Array[Byte], secret: BigInt): Array[Byte] =
    t.mkSigBytes(SigUtils.sign(message, secret))

  private def assertTxFails(
    inputs: Array[InputBox],
    dataInputs: Array[InputBox],
    outputs: Array[org.ergoplatform.appkit.impl.OutBoxImpl],
    secrets: Array[String]
  )(implicit ctx: BlockchainContext): Unit = t.assertTxFails(inputs, dataInputs, outputs, secrets)

  // ==============================================================================================
  // C6 - the unsigned top-up must not be able to drain the box's ERG
  // ==============================================================================================

  property("C6: a 1-unit top-up cannot take the reserve box's ERG") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val ergInReserve = 20000000000L // 20 ERG parked in the token reserve
      val keep = 1000000L             // attacker leaves 0.001 ERG behind
      val held = 1000000L            // reserve token units held by the reserve

      val reserveIn = ctx.newTxBuilder().outBoxBuilder
        .value(ergInReserve)
        .tokens(new ErgoToken(t.reserveNFTBytes, 1), new ErgoToken(t.reserveTokenIdBytes, held))
        .registers(ErgoValue.of(t.ownerPk), freshReserveMap.ergoValue, ErgoValue.of(t.trackerNFTBytes))
        .contract(ctx.compileContract(ConstantsBuilder.empty(), BasisConstants.basisTokenContract))
        .build()
        .convertToInputWith("aa" * 32, 0.toShort)
        .withContextVars(new ContextVar(0, ErgoValue.of(10: Byte))) // action #1, no signature needed

      // The attacker adds exactly 1 token unit (the minimum top-up) and keeps `ergInReserve - keep`
      // of the reserve's ERG in their own output.
      val funding = ctx.newTxBuilder().outBoxBuilder
        .value(keep)
        .tokens(new ErgoToken(t.reserveTokenIdBytes, 1))
        .contract(ctx.compileContract(ConstantsBuilder.empty(), "{sigmaProp(true)}"))
        .build()
        .convertToInputWith("bb" * 32, 0.toShort)

      val reserveOut = t.createOut(
        BasisConstants.basisTokenContract, keep,
        Array(ErgoValue.of(t.ownerPk), freshReserveMap.ergoValue, ErgoValue.of(t.trackerNFTBytes)),
        Array(new ErgoToken(t.reserveNFTBytes, 1), new ErgoToken(t.reserveTokenIdBytes, held + 1))
      )
      val loot = t.createOut(t.trueScript, ergInReserve - keep, Array(), Array())

      assertTxFails(
        Array(reserveIn, funding),
        Array(),
        Array(reserveOut, loot),
        Array()
      )
    }
  }

  property("C6: an honest top-up that keeps the ERG still works") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val ergInReserve = 20000000000L
      val added = 1000000000L
      val held = 1000000L

      val reserveIn = ctx.newTxBuilder().outBoxBuilder
        .value(ergInReserve)
        .tokens(new ErgoToken(t.reserveNFTBytes, 1), new ErgoToken(t.reserveTokenIdBytes, held))
        .registers(ErgoValue.of(t.ownerPk), freshReserveMap.ergoValue, ErgoValue.of(t.trackerNFTBytes))
        .contract(ctx.compileContract(ConstantsBuilder.empty(), BasisConstants.basisTokenContract))
        .build()
        .convertToInputWith("aa" * 32, 0.toShort)
        .withContextVars(new ContextVar(0, ErgoValue.of(10: Byte)))

      // Funds both the top-up and the transaction fee.
      val funding = ctx.newTxBuilder().outBoxBuilder
        .value(added + t.feeValue)
        .tokens(new ErgoToken(t.reserveTokenIdBytes, 1000))
        .contract(ctx.compileContract(ConstantsBuilder.empty(), "{sigmaProp(true)}"))
        .build()
        .convertToInputWith("bb" * 32, 0.toShort)

      // The reserve's ERG goes UP by exactly `added` (funded by the separate input), so ergPreserved
      // holds and this is the shape the tracker's own builder produces.
      val reserveOut = t.createOut(
        BasisConstants.basisTokenContract, ergInReserve + added,
        Array(ErgoValue.of(t.ownerPk), freshReserveMap.ergoValue, ErgoValue.of(t.trackerNFTBytes)),
        Array(new ErgoToken(t.reserveNFTBytes, 1), new ErgoToken(t.reserveTokenIdBytes, held + 1000))
      )

      t.assertTxSucceeds(Array(reserveIn, funding), Array(), Array(reserveOut), Array())
      ()
    }
  }

  // ==============================================================================================
  // C6 - redemption must not drain ERG either, and must not mint reserve NFTs
  // ==============================================================================================

  property("C6: a redemption cannot take the reserve box's ERG") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val ergInReserve = 20000000000L
      val reserveTokenAmount = 1000000000L
      val redeemAmount = 1000000L
      val totalDebt = reserveTokenAmount
      val timestamp = System.currentTimeMillis()

      val key = Blake2b256(t.ownerPk.getEncoded.toArray ++ t.receiverPk.getEncoded.toArray)
      val message = key ++ Longs.toByteArray(totalDebt) ++ Longs.toByteArray(timestamp)
      val reserveSig = signAll(message, t.ownerSecret)
      val trackerSig = signAll(message, t.trackerSecret)

      val map = freshReserveMap
      val inputTree = map.ergoValue
      val insertProof = map.insertOrUpdate(key -> reserveValue(timestamp, redeemAmount)).proof.bytes
      val outputTree = map.ergoValue
      val tracker = t.mkTrackerTreeAndProof(key, totalDebt)

      val reserveIn = ctx.newTxBuilder().outBoxBuilder
        .value(ergInReserve)
        .tokens(new ErgoToken(t.reserveNFTBytes, 1), new ErgoToken(t.reserveTokenIdBytes, reserveTokenAmount))
        .registers(ErgoValue.of(t.ownerPk), inputTree, ErgoValue.of(t.trackerNFTBytes))
        .contract(ctx.compileContract(ConstantsBuilder.empty(), BasisConstants.basisTokenContract))
        .build()
        .convertToInputWith("aa" * 32, 0.toShort)
        .withContextVars(
          new ContextVar(0, ErgoValue.of(0: Byte)),
          new ContextVar(1, ErgoValue.of(t.receiverPk)),
          new ContextVar(2, ErgoValue.of(reserveSig)),
          new ContextVar(3, ErgoValue.of(totalDebt)),
          new ContextVar(4, ErgoValue.of(timestamp)),
          new ContextVar(5, ErgoValue.of(insertProof)),
          new ContextVar(6, ErgoValue.of(trackerSig)),
          new ContextVar(8, ErgoValue.of(tracker.lookupProofBytes))
        )

      // Legitimate in tokens (redeemAmount moved to the creditor) but the reserve's ERG is drained.
      val reserveOut = t.createOut(
        BasisConstants.basisTokenContract, 1000000L,
        Array(ErgoValue.of(t.ownerPk), outputTree, ErgoValue.of(t.trackerNFTBytes)),
        Array(new ErgoToken(t.reserveNFTBytes, 1), new ErgoToken(t.reserveTokenIdBytes, reserveTokenAmount - redeemAmount))
      )
      val creditorOut = t.createOut(
        t.trueScript, t.minValue, Array(),
        Array(new ErgoToken(t.reserveTokenIdBytes, redeemAmount))
      )
      val funding = ctx.newTxBuilder().outBoxBuilder
        .value(t.minValue)
        .contract(ctx.compileContract(ConstantsBuilder.empty(), "{sigmaProp(true)}"))
        .build()
        .convertToInputWith("cc" * 32, 0.toShort)

      assertTxFails(
        Array(reserveIn, funding),
        Array(t.mkTrackerDataInput(tracker.tree)),
        Array(reserveOut, creditorOut),
        Array(t.receiverSecret.toString())
      )
    }
  }

  property("C6: an output cannot inflate the reserve NFT amount") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val ergInReserve = 20000000000L
      val held = 1000000L
      val added = 1000L

      val reserveIn = ctx.newTxBuilder().outBoxBuilder
        .value(ergInReserve)
        .tokens(new ErgoToken(t.reserveNFTBytes, 1), new ErgoToken(t.reserveTokenIdBytes, held))
        .registers(ErgoValue.of(t.ownerPk), freshReserveMap.ergoValue, ErgoValue.of(t.trackerNFTBytes))
        .contract(ctx.compileContract(ConstantsBuilder.empty(), BasisConstants.basisTokenContract))
        .build()
        .convertToInputWith("aa" * 32, 0.toShort)
        .withContextVars(new ContextVar(0, ErgoValue.of(10: Byte)))

      // The funding box supplies 999 reserve-NFT units and tops up the reserve token. Token totals then
      // balance exactly (NFT 1 + 999 in, 1000 out; reserve token held + 1000 in, held + 1000 out), so
      // the builder produces no token change and the ONLY thing wrong with the transaction is that the
      // reserve's NFT amount went from 1 to 1000.
      val funding = ctx.newTxBuilder().outBoxBuilder
        .value(added + t.feeValue)
        .tokens(new ErgoToken(t.reserveTokenIdBytes, 1000), new ErgoToken(t.reserveNFTBytes, 999))
        .contract(ctx.compileContract(ConstantsBuilder.empty(), "{sigmaProp(true)}"))
        .build()
        .convertToInputWith("bb" * 32, 0.toShort)

      val reserveOut = t.createOut(
        BasisConstants.basisTokenContract, ergInReserve + added,
        Array(ErgoValue.of(t.ownerPk), freshReserveMap.ergoValue, ErgoValue.of(t.trackerNFTBytes)),
        // reserve NFT 1 -> 1000 (the attack), reserve token held -> held + 1000 (a normal top-up)
        Array(new ErgoToken(t.reserveNFTBytes, 1000), new ErgoToken(t.reserveTokenIdBytes, held + 1000))
      )

      assertTxFails(Array(reserveIn, funding), Array(), Array(reserveOut), Array())
    }
  }
}
