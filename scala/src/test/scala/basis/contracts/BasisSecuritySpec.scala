package basis.contracts

import basis.offchain.SigUtils
import basis.offchain.SigUtils._
import com.google.common.primitives.Longs
import org.ergoplatform.appkit.impl.OutBoxImpl
import org.ergoplatform.appkit.{
  BlockchainContext, ConstantsBuilder, ContextVar, ErgoValue, InputBox
}
import org.ergoplatform.sdk.ErgoToken
import org.scalatest.{Matchers, PropSpec}
import scorex.crypto.hash.Blake2b256
import sigma.AvlTree
import sigma.data.AvlTreeFlags
import work.lithos.plasma.PlasmaParameters
import work.lithos.plasma.collections.PlasmaMap

/**
  * Security regression tests for the contract findings in `specs/pr14_triage.md`.
  *
  * Unlike the PoC specs that accompanied PR #14, every property here asserts that the attack is
  * **REJECTED**. They run against the real `contract/basis.es` and `contract/basis-token.es` via
  * `BasisConstants`, so they fail if a fix is reverted.
  *
  * Covers:
  *   C1 - replay of a fully redeemed note by omitting context var #7
  *   C2 - two reserve inputs satisfied by a single output (unsigned top-up and redemption)
  *   C5 - the remainder of a partially redeemed note must be claimable
  *   C6 - basis-token.es must not let the box's ERG be drained on redemption or top-up
  *
  * The first-redemption and refund controls at the bottom guard against over-tightening.
  */
class BasisSecuritySpec extends PropSpec with Matchers with basis.HttpClientTesting {

  private val b = new BasisSpec

  private val reserveParams = PlasmaParameters(32, None)
  private val reserveFlags = AvlTreeFlags(insertAllowed = true, updateAllowed = true, removeAllowed = false)

  private def freshReserveMap = new PlasmaMap[Array[Byte], Array[Byte]](reserveFlags, reserveParams)

  /** (timestamp, cumulativeRedeemed) as the reserve tree stores it. */
  private def reserveValue(ts: Long, redeemed: Long): Array[Byte] =
    Longs.toByteArray(ts) ++ Longs.toByteArray(redeemed)

  /** Sign `message` and serialize the `(point, z)` pair the way the contract expects it. */
  private def signAll(message: Array[Byte], secret: BigInt): Array[Byte] =
    b.mkSigBytes(SigUtils.sign(message, secret))

  /**
   * Build a reserve input box under an explicit box id.
   *
   * `BasisSpec.mkBasisInput` hardcodes `fakeTxId1`, so two reserve inputs in one transaction would be
   * rejected by the builder as duplicates. The C2 tests need two distinct reserve boxes.
   */
  private def reserveInput(
    value: Long,
    tree: ErgoValue[AvlTree],
    txId: String,
    vars: Seq[ContextVar]
  )(implicit ctx: BlockchainContext): InputBox =
    ctx.newTxBuilder().outBoxBuilder
      .value(value)
      .tokens(new ErgoToken(b.basisTokenId, 1))
      .registers(ErgoValue.of(b.ownerPk), tree, ErgoValue.of(b.trackerNFTBytes))
      .contract(ctx.compileContract(ConstantsBuilder.empty(), BasisConstants.basisContract))
      .build()
      .convertToInputWith(txId, 0.toShort)
      .withContextVars(vars: _*)

  /** Plain `sigmaProp(true)` input, used to fund outputs and fees. */
  private def fundingBox(value: Long, txId: String)(implicit ctx: BlockchainContext): InputBox =
    ctx.newTxBuilder().outBoxBuilder
      .value(value)
      .contract(ctx.compileContract(ConstantsBuilder.empty(), "{sigmaProp(true)}"))
      .build()
      .convertToInputWith(txId, 0.toShort)

  // ==============================================================================================
  // C1 - replay by omitting context var #7
  // ==============================================================================================

  property("C1: a fully redeemed note cannot be replayed by omitting context var #7") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val totalDebt = 10000000000L
      val timestamp = System.currentTimeMillis()
      val key = Blake2b256(b.ownerPk.getEncoded.toArray ++ b.receiverPk.getEncoded.toArray)
      val message = key ++ Longs.toByteArray(totalDebt) ++ Longs.toByteArray(timestamp)
      val reserveSig = signAll(message, b.ownerSecret)
      val trackerSig = signAll(message, b.trackerSecret)
      val tracker = b.mkTrackerTreeAndProof(key, totalDebt)

      // The reserve tree says this note was ALREADY fully redeemed at this timestamp.
      val map = freshReserveMap
      map.insertOrUpdate(key -> reserveValue(timestamp, totalDebt))
      val inputTree = map.ergoValue
      val lookup = map.lookUp(key).proof.bytes

      // The attacker re-submits the same note WITHOUT #7, hoping the contract reads
      // storedTimestamp/redeemedDebt as 0. The output tree rewrites the same entry.
      val insertProof = map
        .insertOrUpdate(key -> reserveValue(timestamp, totalDebt))
        .proof
        .bytes
      val outputTree = map.ergoValue

      val vars = Seq(
        new ContextVar(0, ErgoValue.of(0: Byte)),
        new ContextVar(1, ErgoValue.of(b.receiverPk)),
        new ContextVar(2, ErgoValue.of(reserveSig)),
        new ContextVar(3, ErgoValue.of(totalDebt)),
        new ContextVar(4, ErgoValue.of(timestamp)),
        new ContextVar(5, ErgoValue.of(insertProof)),
        new ContextVar(6, ErgoValue.of(trackerSig)),
        new ContextVar(8, ErgoValue.of(tracker.lookupProofBytes))
        // NOTE: no context var 7 -- this is the attack.
      )

      val reserveIn = b.mkBasisInput(
        b.minValue + totalDebt + b.feeValue, inputTree, b.receiverPk, reserveSig, totalDebt,
        insertProof, trackerSig, None, Some(tracker.lookupProofBytes),
        timestamp
      ).withContextVars(vars: _*)

      // redeemed = SELF.value - selfOut.value = totalDebt, so the amount arithmetic is legitimate and
      // the ONLY thing wrong with this transaction is the omitted #7.
      val reserveOut = b.createOut(
        BasisConstants.basisContract, b.minValue + b.feeValue,
        Array(ErgoValue.of(b.ownerPk), outputTree, ErgoValue.of(b.trackerNFTBytes)),
        Array(new ErgoToken(b.basisTokenId, 1))
      )
      val payout = b.createOut(b.trueScript, totalDebt, Array(), Array())

      // THE ATTACK: #7 omitted for a note that is already fully redeemed.
      //
      // Against the unfixed contract this is ACCEPTED -- the omission reads storedTimestamp and
      // redeemedDebt as 0, leaving a full totalDebt of headroom, and insertOrUpdate overwrites the
      // entry. Against the fix, the contract takes the `insert` branch and sigma yields None because
      // the key is present, so the `.get` throws.
      assertTxFails(
        Array(reserveIn),
        Array(b.mkTrackerDataInput(tracker.tree)),
        Array(reserveOut, payout),
        Array(b.receiverSecret.toString())
      )

      // Control: the same transaction WITH #7 supplied is also rejected, because the note is already
      // fully redeemed and leaves zero headroom. This shows the fix did not simply reject the shape.
      assertTxFails(
        Array(reserveIn.withContextVars((vars :+ new ContextVar(7, ErgoValue.of(lookup))): _*)),
        Array(b.mkTrackerDataInput(tracker.tree)),
        Array(reserveOut, payout),
        Array(b.receiverSecret.toString())
      )
    }
  }

  property("C1: a genuine first redemption with #7 omitted still succeeds") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val totalDebt = 10000000000L
      val timestamp = System.currentTimeMillis()
      val key = Blake2b256(b.ownerPk.getEncoded.toArray ++ b.receiverPk.getEncoded.toArray)
      val message = key ++ Longs.toByteArray(totalDebt) ++ Longs.toByteArray(timestamp)
      val reserveSig = signAll(message, b.ownerSecret)
      val trackerSig = signAll(message, b.trackerSecret)
      val tracker = b.mkTrackerTreeAndProof(key, totalDebt)

      val map = freshReserveMap
      val inputTree = map.ergoValue
      val insertProof = map.insertOrUpdate(key -> reserveValue(timestamp, totalDebt)).proof.bytes
      val outputTree = map.ergoValue

      val reserveIn = b.mkBasisInput(
        b.minValue + totalDebt + b.feeValue, inputTree, b.receiverPk, reserveSig, totalDebt,
        insertProof, trackerSig, None, Some(tracker.lookupProofBytes),
        timestamp
      )

      // redeemed = SELF.value - selfOut.value = totalDebt, which is exactly the debt headroom.
      val reserveOut = b.createOut(
        BasisConstants.basisContract, b.minValue + b.feeValue,
        Array(ErgoValue.of(b.ownerPk), outputTree, ErgoValue.of(b.trackerNFTBytes)),
        Array(new ErgoToken(b.basisTokenId, 1))
      )
      val payout = b.createOut(b.trueScript, totalDebt, Array(), Array())

      assertTxSucceeds(Array(reserveIn), Array(b.mkTrackerDataInput(tracker.tree)), Array(reserveOut, payout), Array(b.receiverSecret.toString()))
    }
  }

  // ==============================================================================================
  // C2 - two reserve inputs satisfied by one output
  // ==============================================================================================

  property("C2: two reserves of the same owner cannot be merged by the unsigned top-up") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val v1 = 50000000000L
      val v2 = 40000000000L
      val topUp = 100000000L
      val actionVars = Seq(new ContextVar(0, ErgoValue.of(10: Byte))) // action #1, both point at output 0

      def reserve(value: Long, txId: String)(implicit ctx: BlockchainContext): InputBox =
        ctx.newTxBuilder().outBoxBuilder
          .value(value)
          .tokens(new ErgoToken(b.basisTokenId, 1))
          .registers(ErgoValue.of(b.ownerPk), freshReserveMap.ergoValue, ErgoValue.of(b.trackerNFTBytes))
          .contract(ctx.compileContract(ConstantsBuilder.empty(), BasisConstants.basisContract))
          .build()
          .convertToInputWith(txId, 0.toShort)
          .withContextVars(actionVars: _*)

      // One output worth max + 0.1 ERG satisfies both inputs; the attacker keeps the difference.
      val merged = b.createOut(
        BasisConstants.basisContract, v1 + topUp,
        Array(ErgoValue.of(b.ownerPk), freshReserveMap.ergoValue, ErgoValue.of(b.trackerNFTBytes)),
        Array(new ErgoToken(b.basisTokenId, 1))
      )
      val loot = b.createOut(b.trueScript, v2 - topUp, Array(), Array(new ErgoToken(b.basisTokenId, 1)))

      assertTxFails(
        Array(reserve(v1, "aa" * 32), reserve(v2, "bb" * 32), fundingBox(v2, "cc" * 32)),
        Array(),
        Array(merged, loot),
        Array()
      )
    }
  }

  property("C2: two reserves of the same owner cannot both redeem against one output") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val totalDebt = 10000000000L
      val timestamp = System.currentTimeMillis()
      val key = Blake2b256(b.ownerPk.getEncoded.toArray ++ b.receiverPk.getEncoded.toArray)
      val message = key ++ Longs.toByteArray(totalDebt) ++ Longs.toByteArray(timestamp)
      val reserveSig = signAll(message, b.ownerSecret)
      val trackerSig = signAll(message, b.trackerSecret)

      val map = freshReserveMap
      val inputTree = map.ergoValue
      val insertProof = map.insertOrUpdate(key -> reserveValue(timestamp, totalDebt)).proof.bytes
      val outputTree = map.ergoValue
      val tracker = b.mkTrackerTreeAndProof(key, totalDebt)

      // Both reserves select OUTPUTS(0), so each must be a distinct box.
      val sharedVars = Seq(
        new ContextVar(0, ErgoValue.of(0: Byte)),
        new ContextVar(1, ErgoValue.of(b.receiverPk)),
        new ContextVar(2, ErgoValue.of(reserveSig)),
        new ContextVar(3, ErgoValue.of(totalDebt)),
        new ContextVar(4, ErgoValue.of(timestamp)),
        new ContextVar(5, ErgoValue.of(insertProof)),
        new ContextVar(6, ErgoValue.of(trackerSig)),
        new ContextVar(8, ErgoValue.of(tracker.lookupProofBytes))
      )
      def reserve(value: Long, txId: String)(implicit ctx: BlockchainContext): InputBox =
        reserveInput(value, inputTree, txId, sharedVars)

      // Both inputs select OUTPUTS(0). Chosen so that EACH input on its own satisfies every contract
      // condition: redeemed = 100 ERG - 90 ERG = 10 ERG, exactly the debt headroom. The only thing
      // wrong with this transaction is that two reserve inputs share one output, so if the merge were
      // possible it would succeed -- and the creditor would collect 110 ERG for a 10 ERG note.
      val sharedOut = b.createOut(
        BasisConstants.basisContract, 90000000000L,
        Array(ErgoValue.of(b.ownerPk), outputTree, ErgoValue.of(b.trackerNFTBytes)),
        Array(new ErgoToken(b.basisTokenId, 1))
      )
      val payout = b.createOut(b.trueScript, 110000000000L, Array(), Array(new ErgoToken(b.basisTokenId, 1)))

      assertTxFails(
        Array(reserve(100000000000L, "aa" * 32), reserve(100000000000L, "bb" * 32)),
        Array(b.mkTrackerDataInput(tracker.tree)),
        Array(sharedOut, payout),
        Array(b.receiverSecret.toString())
      )
    }
  }

  property("C2 control: the one-reserve-per-owner limit is per transaction, not per owner") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val totalDebt = 1000000000L
      val timestamp = System.currentTimeMillis()

      def redeemOnce(txId: String)(implicit ctx: BlockchainContext): Unit = {
        val key = Blake2b256(b.ownerPk.getEncoded.toArray ++ b.receiverPk.getEncoded.toArray)
        val message = key ++ Longs.toByteArray(totalDebt) ++ Longs.toByteArray(timestamp)
        val reserveSig = signAll(message, b.ownerSecret)
        val trackerSig = signAll(message, b.trackerSecret)
        val tracker = b.mkTrackerTreeAndProof(key, totalDebt)

        val map = freshReserveMap
        val inputTree = map.ergoValue
        val insertProof = map.insertOrUpdate(key -> reserveValue(timestamp, totalDebt)).proof.bytes
        val outputTree = map.ergoValue

        val vars = Seq(
          new ContextVar(0, ErgoValue.of(0: Byte)),
          new ContextVar(1, ErgoValue.of(b.receiverPk)),
          new ContextVar(2, ErgoValue.of(reserveSig)),
          new ContextVar(3, ErgoValue.of(totalDebt)),
          new ContextVar(4, ErgoValue.of(timestamp)),
          new ContextVar(5, ErgoValue.of(insertProof)),
          new ContextVar(6, ErgoValue.of(trackerSig)),
          new ContextVar(8, ErgoValue.of(tracker.lookupProofBytes))
        )

        val inBox = reserveInput(b.minValue + totalDebt, inputTree, txId, vars)
        val outBox = b.createOut(
          BasisConstants.basisContract, b.minValue,
          Array(ErgoValue.of(b.ownerPk), outputTree, ErgoValue.of(b.trackerNFTBytes)),
          Array(new ErgoToken(b.basisTokenId, 1))
        )
        val payout = b.createOut(b.trueScript, totalDebt, Array(), Array())

        assertTxSucceeds(Array(inBox), Array(b.mkTrackerDataInput(tracker.tree)),
          Array(outBox, payout), Array(b.receiverSecret.toString()))
      }

      // `uniqueReserveInput` counts reserve inputs of THIS owner in THIS transaction, so an owner may
      // of course redeem from their reserve again in a later transaction. Two independent
      // redemptions, each spending a single reserve, both work.
      redeemOnce("aa" * 32)
      redeemOnce("bb" * 32)
    }
  }

  // ==============================================================================================
  // C5 - remainder of a partially redeemed note
  // ==============================================================================================

  property("C5: the remainder of a partially redeemed note can be claimed after a top-up") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val totalDebt = 10000000000L
      val timestamp = System.currentTimeMillis()
      val alreadyRedeemed = 4000000000L
      val remainder = totalDebt - alreadyRedeemed

      val key = Blake2b256(b.ownerPk.getEncoded.toArray ++ b.receiverPk.getEncoded.toArray)
      val message = key ++ Longs.toByteArray(totalDebt) ++ Longs.toByteArray(timestamp)
      val reserveSig = signAll(message, b.ownerSecret)
      val trackerSig = signAll(message, b.trackerSecret)

      // The tree records a partial redemption at the SAME timestamp as the note -- which is exactly
      // what the first redemption wrote, so the note is unchanged since.
      val map = freshReserveMap
      map.insertOrUpdate(key -> reserveValue(timestamp, alreadyRedeemed))
      val inputTree = map.ergoValue
      val lookup = map.lookUp(key).proof.bytes
      val insertProof = map.insertOrUpdate(key -> reserveValue(timestamp, totalDebt)).proof.bytes
      val outputTree = map.ergoValue
      val tracker = b.mkTrackerTreeAndProof(key, totalDebt)

      val reserveIn = b.mkBasisInput(
        b.minValue + remainder + b.feeValue, inputTree, b.receiverPk, reserveSig, totalDebt,
        insertProof, trackerSig, Some(lookup), Some(tracker.lookupProofBytes), timestamp
      )
      // redeemed = SELF.value - selfOut.value = remainder = totalDebt - alreadyRedeemed.
      val reserveOut = b.createOut(
        BasisConstants.basisContract, b.minValue + b.feeValue,
        Array(ErgoValue.of(b.ownerPk), outputTree, ErgoValue.of(b.trackerNFTBytes)),
        Array(new ErgoToken(b.basisTokenId, 1))
      )
      val payout = b.createOut(b.trueScript, remainder, Array(), Array())

      assertTxSucceeds(Array(reserveIn), Array(b.mkTrackerDataInput(tracker.tree)), Array(reserveOut, payout), Array(b.receiverSecret.toString()))
    }
  }

  property("C5 control: a note OLDER than the stored timestamp is still rejected") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val totalDebt = 10000000000L
      val oldTimestamp = System.currentTimeMillis() - 60000
      val newTimestamp = System.currentTimeMillis()

      val key = Blake2b256(b.ownerPk.getEncoded.toArray ++ b.receiverPk.getEncoded.toArray)
      val message = key ++ Longs.toByteArray(totalDebt) ++ Longs.toByteArray(oldTimestamp)
      val reserveSig = signAll(message, b.ownerSecret)
      val trackerSig = signAll(message, b.trackerSecret)

      val map = freshReserveMap
      map.insertOrUpdate(key -> reserveValue(newTimestamp, 1000L))
      val inputTree = map.ergoValue
      val lookup = map.lookUp(key).proof.bytes
      val insertProof = map.insertOrUpdate(key -> reserveValue(oldTimestamp, totalDebt)).proof.bytes
      val outputTree = map.ergoValue
      val tracker = b.mkTrackerTreeAndProof(key, totalDebt)

      val reserveIn = b.mkBasisInput(
        b.minValue + totalDebt, inputTree, b.receiverPk, reserveSig, totalDebt,
        insertProof, trackerSig, Some(lookup), Some(tracker.lookupProofBytes), oldTimestamp
      )
      val reserveOut = b.createOut(
        BasisConstants.basisContract, b.minValue,
        Array(ErgoValue.of(b.ownerPk), outputTree, ErgoValue.of(b.trackerNFTBytes)),
        Array(new ErgoToken(b.basisTokenId, 1))
      )
      val payout = b.createOut(b.trueScript, totalDebt, Array(), Array())

      assertTxFails(
        Array(reserveIn),
        Array(b.mkTrackerDataInput(tracker.tree)),
        Array(reserveOut, payout),
        Array(b.receiverSecret.toString())
      )
    }
  }

  // ==============================================================================================
  // Helpers borrowed from BasisSpec
  // ==============================================================================================

  private def assertTxFails(
    inputs: Array[InputBox],
    dataInputs: Array[InputBox],
    outputs: Array[OutBoxImpl],
    secrets: Array[String]
  )(implicit ctx: BlockchainContext): Unit = b.assertTxFails(inputs, dataInputs, outputs, secrets)

  private def assertTxSucceeds(
    inputs: Array[InputBox],
    dataInputs: Array[InputBox],
    outputs: Array[OutBoxImpl],
    secrets: Array[String]
  )(implicit ctx: BlockchainContext): Unit = {
    b.assertTxSucceeds(inputs, dataInputs, outputs, secrets)
    ()
  }
}
