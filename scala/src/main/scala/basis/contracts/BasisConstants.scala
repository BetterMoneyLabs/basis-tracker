package basis.contracts

import basis.offchain.SigUtils._
import org.ergoplatform.ErgoAddressEncoder
import org.ergoplatform.appkit.{AppkitHelpers, ErgoValue, NetworkType}
import scorex.crypto.hash.Blake2b256
import scorex.util.encode.Base58
import sigma.crypto.CryptoConstants
import sigma.data.AvlTreeFlags
import sigma.ast.ErgoTree
import sigma.compiler.{CompilerSettings, SigmaCompiler}
import sigma.ast.TransformingSigmaBuilder
import sigma.{AvlTree, GroupElement}
import work.lithos.plasma.PlasmaParameters
import work.lithos.plasma.collections.PlasmaMap

import java.util

/**
 * Constants and utilities for the Basis reserve contract.
 *
 * This is an extraction of the Basis-specific parts of the chaincash reference
 * implementation, relocated to read contracts from `../contract/` relative to
 * the `scala/` subproject root.
 */
object BasisConstants {

  val networkType: NetworkType = NetworkType.MAINNET
  val networkPrefix: Byte = networkType.networkPrefix
  val ergoAddressEncoder: ErgoAddressEncoder = new ErgoAddressEncoder(networkPrefix)
  private val compiler = SigmaCompiler(CompilerSettings(networkPrefix, TransformingSigmaBuilder, lowerMethodCalls = true))

  def getAddressFromErgoTree(ergoTree: ErgoTree): org.ergoplatform.ErgoAddress =
    ergoAddressEncoder.fromProposition(ergoTree).get

  def substitute(contract: String, substitutionMap: Map[String, String] = Map.empty): String = {
    substitutionMap.foldLeft(contract) { case (c, (k, v)) =>
      c.replace("$" + k, v)
    }
  }

  def readContract(path: String, substitutionMap: Map[String, String] = Map.empty): String = {
    // The scala/ subproject is one level below the repo root, so contracts live in ../contract/
    val contract = scala.io.Source.fromFile("../contract/" + path, "utf-8").getLines.mkString("\n")
    substitute(contract, substitutionMap)
  }

  def compile(ergoScript: String): ErgoTree = {
    // Compile under v6 (blockVersion 4) to enable AvlTree.insertOrUpdate.
    AppkitHelpers.compile(new util.HashMap[String, Object](), ergoScript, networkPrefix, 4: Byte)
  }

  // Basis AVL tree parameters
  // keyLength = 32 (Blake2b256 hash of ownerKey || receiverKey)
  val basisPlasmaParameters: PlasmaParameters = PlasmaParameters(32, None)

  def emptyBasisPlasmaMap: PlasmaMap[Array[Byte], Array[Byte]] =
    new PlasmaMap[Array[Byte], Array[Byte]](AvlTreeFlags.InsertOnly, basisPlasmaParameters)

  val emptyTreeErgoValue: ErgoValue[AvlTree] = emptyBasisPlasmaMap.ergoValue
  val emptyTree: AvlTree = emptyTreeErgoValue.getValue

  val g: GroupElement = CryptoConstants.dlogGroup.generator

  val basisContract: String = readContract("basis.es", Map())
  val basisErgoTree: ErgoTree = compile(basisContract)
  val basisAddress: org.ergoplatform.ErgoAddress = getAddressFromErgoTree(basisErgoTree)
  val basisContractHash: Array[Byte] = Blake2b256(basisErgoTree.bytes.tail)
  val basisContractHashString: String = Base58.encode(basisContractHash)

  val basisTokenContract: String = readContract("basis-token.es", Map())
  val basisTokenErgoTree: ErgoTree = compile(basisTokenContract)
  val basisTokenAddress: org.ergoplatform.ErgoAddress = getAddressFromErgoTree(basisTokenErgoTree)

  // Action codes for Basis contract
  val REDEEM_ACTION: Byte = 0
  val TOP_UP_ACTION: Byte = 1
  val INITIATE_REFUND_ACTION: Byte = 2
  val COMPLETE_REFUND_ACTION: Byte = 3

  // Minimum top-up amount (0.1 ERG)
  val MIN_TOP_UP_AMOUNT: Long = 100000000L

  // Emergency redemption time (3 days in blocks, assuming ~2.5 min per block)
  val EMERGENCY_REDEMPTION_TIME_IN_BLOCKS: Int = 3 * 720

  // Refund waiting period (2 months in blocks, assuming ~2 min per block)
  val REFUND_PERIOD_BLOCKS: Int = 43200
}

object BasisPrinter extends App {
  println("Basis p2s address: " + BasisConstants.basisAddress)
  println("Basis-token p2s address: " + BasisConstants.basisTokenAddress)

  println("\nTo deploy Basis reserve:")
  println("1. Run BasisDeployer.main() for deployment requests")
  println("2. Use createBasisDeploymentRequest() with actual values")
}
