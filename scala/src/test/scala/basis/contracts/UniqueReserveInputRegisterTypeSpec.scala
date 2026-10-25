package basis.contracts

import org.ergoplatform.appkit.{BlockchainContext, ConstantsBuilder, ContextVar, ErgoValue, InputBox}
import org.ergoplatform.sdk.ErgoToken
import org.scalatest.{Matchers, PropSpec}
import sigma.data.AvlTreeFlags
import work.lithos.plasma.PlasmaParameters
import work.lithos.plasma.collections.PlasmaMap

/**
 * Regression spec for the `uniqueReserveInput` guard in contract/basis.es.
 *
 * A previous version of the contract evaluated `in.R4[GroupElement]` on EVERY input of the
 * transaction, so any transaction co-spending a box whose R4 holds a different type (e.g.
 * Coll[Byte] or Long) threw `InvalidType` and failed -- even though such boxes have nothing to
 * do with the reserve. The guard now short-circuits on `in.propositionBytes == SELF.propositionBytes`
 * before touching R4, so non-reserve inputs are skipped regardless of their register types.
 *
 * Both properties below must FAIL if the `in.propositionBytes == SELF.propositionBytes &&`
 * conjunct is removed from `uniqueReserveInput` in contract/basis.es (verified manually).
 */
class UniqueReserveInputRegisterTypeSpec extends PropSpec with Matchers with basis.HttpClientTesting {
  private val b = new BasisSpec
  private val flags = AvlTreeFlags(insertAllowed = true, updateAllowed = true, removeAllowed = false)
  private def emptyTree = new PlasmaMap[Array[Byte], Array[Byte]](flags, PlasmaParameters(32, None)).ergoValue

  private def reserve(value: Long, txId: String)(implicit ctx: BlockchainContext): InputBox =
    ctx.newTxBuilder().outBoxBuilder.value(value)
      .tokens(new ErgoToken(b.basisTokenId, 1))
      .registers(ErgoValue.of(b.ownerPk), emptyTree, ErgoValue.of(b.trackerNFTBytes))
      .contract(ctx.compileContract(ConstantsBuilder.empty(), BasisConstants.basisContract))
      .build().convertToInputWith(txId, 0.toShort)
      .withContextVars(new ContextVar(0, ErgoValue.of(10: Byte))) // action 1 (top-up), index 0

  private def funding(value: Long, txId: String, r4: Option[ErgoValue[_]])(implicit ctx: BlockchainContext): InputBox = {
    val bld = ctx.newTxBuilder().outBoxBuilder.value(value)
      .contract(ctx.compileContract(ConstantsBuilder.empty(), "{sigmaProp(true)}"))
    r4.fold(bld)(v => bld.registers(v)).build().convertToInputWith(txId, 0.toShort)
  }

  private def run(r4: Option[ErgoValue[_]]): Unit =
    createMockedErgoClient(MockData(Nil, Nil)).execute { implicit ctx: BlockchainContext =>
      val v = 10000000000L
      val out = b.createOut(BasisConstants.basisContract, v + 100000000L,
        Array(ErgoValue.of(b.ownerPk), emptyTree, ErgoValue.of(b.trackerNFTBytes)),
        Array(new ErgoToken(b.basisTokenId, 1)))
      b.assertTxSucceeds(Array(reserve(v, "aa" * 32), funding(1000000000L, "cc" * 32, r4)), Array(), Array(out), Array())
    }

  property("top-up co-spending a box with R4 = Coll[Byte]") { run(Some(ErgoValue.of("name".getBytes))) }
  property("top-up co-spending a box with R4 = Long") { run(Some(ErgoValue.of(5L))) }
}
